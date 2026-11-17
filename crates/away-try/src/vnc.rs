//! Tries a machine over VNC with Away's own code, and says what happened at each step.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use away_secret::Secret;
use away_vnc::connection::{Connection, Happened, Input, Passes};
use sha2::{Digest, Sha256};

use crate::extensions::{self, Heard, KNOWN};

pub const USAGE: &str = "\
away-try vnc <address> [--password-file <file>] [--save <file>] [--wheel <turn> --at <x>,<y>]
away-try vnc <address> [--password-file <file>] --extensions [--also <number>,<number>…]

  Reaches a machine over VNC with Away's own code and says what it shows, step by step.
  The address is a name or a number, with a port after a colon where it is not 5900.

  A machine that asks for its password is given the one in the file, so that it is on
  no command line. With --save it writes the desktop's picture to a file, as a BMP,
  once the machine has stopped drawing.

  With --wheel the pointer goes to the place named with --at and the wheel is turned
  there once the desktop rests: 120 is one notch away from the person, -120 one
  towards them. It says how many steps of the wheel that is for this machine.

  With --extensions it says what the machine knows beyond the protocol's core: the ways
  of signing in it offers and, once its desktop is open, which extensions it answers
  when they are named to it. A machine lists none of them by itself. --also names
  further ways of writing a rectangle by their numbers, for a machine with ways of its
  own.
";

/// How long a machine gets to answer, at each step on the way to its desktop.
const WITHIN: Duration = Duration::from_secs(10);
/// How long the machine may draw nothing before its picture counts as the desktop at rest.
const QUIET_FOR: Duration = Duration::from_secs(3);
/// How long the picture is waited for at most, on a desktop that never rests.
const DRAWS_AT_MOST: Duration = Duration::from_secs(45);
/// How long a machine may say nothing new before its answers to the extensions count as all.
const ANSWERS_WITHIN: Duration = Duration::from_secs(4);
/// How long its answers are waited for at most.
const ANSWERS_AT_MOST: Duration = Duration::from_secs(20);

struct Asked {
    address: String,
    password_file: Option<String>,
    save: Option<String>,
    wheel: Option<i16>,
    at: Option<(u16, u16)>,
    extensions: bool,
    also: Vec<i32>,
}

fn asked(words: &[String]) -> Option<Asked> {
    let [address, rest @ ..] = words else { return None };
    if address.starts_with('-') {
        return None;
    }
    let mut asked =
        Asked { address: address.clone(), password_file: None, save: None, wheel: None, at: None, extensions: false, also: Vec::new() };
    // the one option that takes no word after it
    let named = rest.iter().filter(|word| *word == "--extensions").count();
    if named > 1 {
        return None;
    }
    asked.extensions = named == 1;
    let rest: Vec<String> = rest.iter().filter(|word| *word != "--extensions").cloned().collect();
    let mut rest = rest.as_slice();
    while let [option, value, after @ ..] = rest {
        match option.as_str() {
            "--password-file" if asked.password_file.is_none() => asked.password_file = Some(value.clone()),
            "--save" if asked.save.is_none() => asked.save = Some(value.clone()),
            "--wheel" if asked.wheel.is_none() => asked.wheel = Some(value.parse().ok()?),
            "--at" if asked.at.is_none() => {
                let (x, y) = value.split_once(',')?;
                asked.at = Some((x.parse().ok()?, y.parse().ok()?));
            }
            "--also" if asked.also.is_empty() => {
                asked.also = value.split(',').map(|number| number.parse().ok()).collect::<Option<Vec<i32>>>()?;
            }
            _ => return None,
        }
        rest = after;
    }
    // a wheel is turned at a place, and what a machine knows is asked by itself
    let by_itself = asked.save.is_none() && asked.wheel.is_none();
    (rest.is_empty() && asked.wheel.is_some() == asked.at.is_some() && if asked.extensions { by_itself } else { asked.also.is_empty() })
        .then_some(asked)
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
    // the machine's first bytes, in which it says how it signs people in
    let mut first: Vec<u8> = Vec::new();
    let mut said_ways = !asked.extensions;
    let waits_since = Instant::now();
    loop {
        let mut bytes = [0u8; 65_536];
        let arrived = match machine.read(&mut bytes) {
            Ok(0) => return Err("It closed the connection.".to_string()),
            Ok(count) => &bytes[..count],
            Err(error) if matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => &bytes[..0],
            Err(error) => return Err(format!("The connection broke: {error}.")),
        };
        if !said_ways {
            first.extend_from_slice(&arrived[..arrived.len().min(300 - first.len().min(300))]);
        }
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
            // how it signs people in is said once it is known, before what came of it
            if !said_ways && !matches!(happened, Happened::Speaks(_)) {
                said_ways = true;
                if let Some(ways) = extensions::ways_of_signing_in(&first) {
                    println!("   It offers {} of signing in:", if ways.len() == 1 { "one way" } else { "these ways" });
                    for way in ways {
                        println!("      {way}: {}", extensions::way_of_signing_in(way));
                    }
                }
            }
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
                    if asked.extensions {
                        return knows(&mut machine, width, height, &asked.also);
                    }
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

/// Names every extension to a machine whose desktop is open, and says which it answered.
fn knows(machine: &mut TcpStream, width: u16, height: u16, also: &[i32]) -> Result<(), String> {
    println!("Its extensions");
    match also.len() {
        0 => println!("   Naming it the {} this program reads, and waiting for what it answers", KNOWN.len()),
        more => println!("   Naming it the {} this program reads and {more} more by number, and waiting for what it answers", KNOWN.len()),
    }
    machine.write_all(&extensions::asks(also, width, height)).map_err(|error| format!("It couldn't be sent: {error}."))?;
    let mut heard = Heard::default();
    let began = Instant::now();
    let (mut last_new, mut known) = (began, 0);
    let mut closed = false;
    while heard.stopped.is_none() && last_new.elapsed() < ANSWERS_WITHIN && began.elapsed() < ANSWERS_AT_MOST {
        let mut bytes = [0u8; 65_536];
        match machine.read(&mut bytes) {
            Ok(0) => {
                closed = true;
                break;
            }
            Ok(count) => heard.receive(&bytes[..count]),
            Err(error) if matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(error) => return Err(format!("The connection broke: {error}.")),
        }
        if heard.answered.len() != known {
            (last_new, known) = (Instant::now(), heard.answered.len());
        }
    }
    let answered = |number: &i32| heard.answered.contains_key(number);
    if KNOWN.iter().any(|known| answered(&known.number)) {
        println!("   It answered these:");
        for known in KNOWN.iter().filter(|known| answered(&known.number)) {
            let said = &heard.answered[&known.number];
            println!(
                "      {} ({}): it {}.{}",
                known.name,
                known.number,
                known.does,
                if said.is_empty() { String::new() } else { format!(" It said: {said}.") }
            );
        }
    } else {
        println!("   It answered none of them.");
    }
    let silent: Vec<_> = KNOWN.iter().filter(|known| !answered(&known.number)).collect();
    if !silent.is_empty() {
        println!("   It said nothing to these:");
        for known in silent {
            let another_way = extensions::POINTERS.contains(&known.number) && extensions::POINTERS.iter().any(answered);
            let (name, number) = (known.name, known.number);
            match (another_way, known.only) {
                (true, _) => {
                    println!("      {name} ({number}): it sent the pointer's shape in another of these ways, so this is no answer.")
                }
                (false, Some(only)) => {
                    println!("      {name} ({number}): a machine that knows it says so only {only}, so this is no answer.")
                }
                (false, None) => println!("      {name} ({number}): it gave no answer, where a machine that knows it answers at once."),
            }
        }
    }
    for (number, said) in heard.answered.iter().filter(|(number, _)| KNOWN.iter().all(|known| known.number != **number)) {
        println!("   It also answered one numbered {number}{}.", if said.is_empty() { String::new() } else { format!(": {said}") });
    }
    if !heard.drew_as.is_empty() {
        let ways: Vec<String> = heard.drew_as.iter().map(i32::to_string).collect();
        println!("   It wrote its pixels in the ways numbered {}.", ways.join(", "));
    }
    if let Some(length) = heard.plain_text {
        println!("   It sent what its clipboard holds the plain way, as western text: {length} bytes.");
    }
    if let Some(why) = &heard.stopped {
        println!("   {why}");
        println!("   Nothing after that was read.");
    }
    if closed {
        println!("   It closed the connection.");
    }
    Ok(())
}
