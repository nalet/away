//! Tries a machine over VNC with Away's own code, and says what happened at each step.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use away_secret::Secret;
use away_vnc::connection::{Connection, Happened, Input, Passes};
use sha2::{Digest, Sha256};

pub const USAGE: &str = "\
away-try vnc <address> [--password-file <file>] [--save <file>] [--wheel <turn> --at <x>,<y>]

  Reaches a machine over VNC with Away's own code and says what it shows, step by step.
  The address is a name or a number, with a port after a colon where it is not 5900.

  A machine that asks for its password is given the one in the file, so that it is on
  no command line. With --save it writes the desktop's picture to a file, as a BMP,
  once the machine has stopped drawing.

  With --wheel the pointer goes to the place named with --at and the wheel is turned
  there once the desktop rests: 120 is one notch away from the person, -120 one
  towards them. It says how many steps of the wheel that is for this machine.
";

/// How long a machine gets to answer, at each step on the way to its desktop.
const WITHIN: Duration = Duration::from_secs(10);
/// How long the machine may draw nothing before its picture counts as the desktop at rest.
const QUIET_FOR: Duration = Duration::from_secs(3);
/// How long the picture is waited for at most, on a desktop that never rests.
const DRAWS_AT_MOST: Duration = Duration::from_secs(45);

struct Asked {
    address: String,
    password_file: Option<String>,
    save: Option<String>,
    wheel: Option<i16>,
    at: Option<(u16, u16)>,
}

fn asked(words: &[String]) -> Option<Asked> {
    let [address, rest @ ..] = words else { return None };
    if address.starts_with('-') {
        return None;
    }
    let mut asked = Asked { address: address.clone(), password_file: None, save: None, wheel: None, at: None };
    let mut rest = rest;
    while let [option, value, after @ ..] = rest {
        match option.as_str() {
            "--password-file" if asked.password_file.is_none() => asked.password_file = Some(value.clone()),
            "--save" if asked.save.is_none() => asked.save = Some(value.clone()),
            "--wheel" if asked.wheel.is_none() => asked.wheel = Some(value.parse().ok()?),
            "--at" if asked.at.is_none() => {
                let (x, y) = value.split_once(',')?;
                asked.at = Some((x.parse().ok()?, y.parse().ok()?));
            }
            _ => return None,
        }
        rest = after;
    }
    // a wheel is turned at a place
    (rest.is_empty() && asked.wheel.is_some() == asked.at.is_some()).then_some(asked)
}

pub fn run(words: &[String]) -> Option<Result<(), String>> {
    let asked = asked(words)?;
    Some(tried(&asked))
}

fn tried(asked: &Asked) -> Result<(), String> {
    let password = match &asked.password_file {
        Some(file) => {
            let read = std::fs::read_to_string(file).map_err(|error| format!("The password's file can't be read: {error}."))?;
            Some(Secret::new(read.trim_end_matches(['\r', '\n'])))
        }
        None => None,
    };
    // a number of the newer kind is written in brackets, and a port follows a colon
    let (host, port) = match asked.address.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() && !host.ends_with(':') && port.parse::<u16>().is_ok() => {
            (host, port.parse().unwrap_or(5900))
        }
        _ => (asked.address.as_str(), 5900),
    };
    let host = host.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')).unwrap_or(host);
    println!("Reaching {host}, on port {port}");
    let to = (host, port)
        .to_socket_addrs()
        .map_err(|error| format!("{host} leads to no address: {error}."))?
        .next()
        .ok_or_else(|| format!("{host} leads to no address."))?;
    let began = Instant::now();
    let mut machine = TcpStream::connect_timeout(&to, WITHIN).map_err(|error| format!("It could not be reached: {error}."))?;
    println!("   It answered in {} ms.", began.elapsed().as_millis());
    machine.set_nodelay(true).ok();
    machine.set_read_timeout(Some(Duration::from_millis(250))).map_err(|error| error.to_string())?;

    let mut connection = Connection::begin(password, Passes::default());
    let (mut opened, mut last_drawn): (Option<Instant>, Option<Instant>) = (None, None);
    let (mut rectangles, mut pixels) = (0usize, 0usize);
    let mut turned = false;
    let waits_since = Instant::now();
    loop {
        let mut bytes = [0u8; 65_536];
        let arrived = match machine.read(&mut bytes) {
            Ok(0) => return Err("It closed the connection.".to_string()),
            Ok(count) => &bytes[..count],
            Err(error) if matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => &bytes[..0],
            Err(error) => return Err(format!("The connection broke: {error}.")),
        };
        let (send, happened) = connection.receive(arrived).map_err(|error| {
            let mut says = error.to_string();
            if let Some(first) = says.get_mut(..1) {
                first.make_ascii_uppercase();
            }
            format!("{says}.")
        })?;
        if !send.is_empty() {
            machine.write_all(&send).map_err(|error| format!("It couldn't be sent: {error}."))?;
        }
        for happened in happened {
            match happened {
                Happened::Speaks(version) => {
                    println!("What it speaks");
                    println!(
                        "   Version {version} of VNC{}.",
                        if connection.is_a_mac() { ", which is a Mac's own screen sharing" } else { "" }
                    );
                    println!("Signing in");
                }
                Happened::Asks => {
                    println!("   It asks for its password, and none was given: name its file with --password-file.");
                    return Ok(());
                }
                Happened::SignedIn => println!("   It took the client."),
                Happened::Open { width, height, name } => {
                    println!("Its desktop");
                    println!("   {width} by {height} pixels, called “{name}”.");
                    println!("Taking what it draws, until it has drawn nothing for {} seconds", QUIET_FOR.as_secs());
                    opened = Some(Instant::now());
                }
                Happened::Resized { width, height } => println!("   Its desktop is {width} by {height} pixels now."),
                Happened::Drawn(areas) => {
                    rectangles += areas.len();
                    pixels += areas.iter().map(|area| area.width * area.height).sum::<usize>();
                    last_drawn = Some(Instant::now());
                }
                Happened::Copied(_) => {}
            }
        }
        let now = Instant::now();
        if opened.is_none() && now.duration_since(waits_since) >= WITHIN {
            return Err(format!("It gave no answer within {} seconds.", WITHIN.as_secs()));
        }
        let quiet = last_drawn.is_some_and(|last| now.duration_since(last) >= QUIET_FOR);
        // the wheel is turned once the desktop rests, and what the machine draws then is taken too
        if let (true, false, Some(by), Some((x, y))) = (quiet, turned, asked.wheel, asked.at) {
            turned = true;
            let told = connection.input(&[Input::Moved { x, y }, Input::Wheel { by, sideways: false, x, y }]);
            println!("Turning the wheel by {by} at {x}, {y}");
            // the pointer's move is one message of six bytes, and a step two of them
            println!("   That is {} steps of the wheel for this machine.", (told.len() / 6).saturating_sub(1) / 2);
            machine.write_all(&told).map_err(|error| format!("It couldn't be sent: {error}."))?;
            last_drawn = Some(now);
            continue;
        }
        if opened.is_some_and(|opened| quiet || now.duration_since(opened) >= DRAWS_AT_MOST) {
            if !quiet {
                println!("   It still drew after {} seconds, which is where this stops.", DRAWS_AT_MOST.as_secs());
            }
            break;
        }
    }
    println!("   {rectangles} rectangles, {pixels} pixels in all.");
    let picture = connection.picture();
    println!("The desktop's picture");
    let mut sum = Sha256::new();
    picture.pixels.as_chunks::<4>().0.iter().for_each(|pixel| sum.update(&pixel[..3]));
    let sum: String = sum.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
    println!("   {} by {} pixels. Their SHA-256: {sum}", picture.width, picture.height);
    match &asked.save {
        Some(file) => {
            let picture = away_rdp::picture::Picture { width: picture.width, height: picture.height, pixels: picture.pixels.clone() };
            std::fs::write(file, crate::bmp(&picture)).map_err(|error| format!("The picture couldn't be written to {file}: {error}."))?;
            println!("   It is in {file}.");
        }
        None => println!("   To keep it, name a file: --save <file>."),
    }
    Ok(())
}
