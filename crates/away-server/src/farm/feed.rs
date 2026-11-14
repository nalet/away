//! What a farm of Windows machines publishes, as its own feed says it: the desktops and
//! the programs, each with its icon and with the file that says how it is reached (the
//! feed of MS-TSWP, which a Windows farm's web access gives).

use super::xml::{self, Element};

/// The most a feed may list.
const MOST: usize = 2000;

/// One thing a farm publishes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource {
    /// The name people know it by.
    pub title: String,
    /// The name the farm knows it by.
    pub alias: String,
    /// Whether it is a desktop. Everything else is a program.
    pub desktop: bool,
    /// When the farm last changed it, as the farm writes it.
    pub changed: String,
    /// Where its icon is, as a PNG picture, on the farm's web access.
    pub icon: Option<String>,
    /// Where the file is that says how it is reached.
    pub file: Option<String>,
}

/// Reads a feed.
pub fn read(text: &str) -> Result<Vec<Resource>, String> {
    let feed = xml::read(text)?;
    if feed.name != "ResourceCollection" {
        return Err("it is not a feed of what a farm publishes".to_string());
    }
    let listed = feed.all("Resource");
    if listed.len() > MOST {
        return Err(format!("it lists more than {MOST} things"));
    }
    let mut resources: Vec<Resource> = Vec::with_capacity(listed.len());
    for resource in listed {
        let (Some(title), Some(alias)) = (resource.attribute("Title"), resource.attribute("Alias")) else {
            return Err("it lists something without a name".to_string());
        };
        let title = title.trim();
        if title.is_empty() || title.chars().count() > 120 || title.chars().any(char::is_control) {
            return Err("it lists something whose name is empty, or longer than a name is".to_string());
        }
        // the same name twice: the first one stands
        if resources.iter().any(|other| other.title.eq_ignore_ascii_case(title)) {
            continue;
        }
        resources.push(Resource {
            title: title.to_string(),
            alias: alias.to_string(),
            desktop: resource.attribute("Type") == Some("Desktop"),
            changed: resource.attribute("LastUpdated").unwrap_or_default().to_string(),
            icon: icon(resource),
            file: resource.all("ResourceFile").into_iter().find_map(|file| path(file.attribute("URL")?)),
        });
    }
    Ok(resources)
}

/// A path on the farm's own web access: nothing that leads elsewhere.
fn path(written: &str) -> Option<String> {
    (written.starts_with('/')
        && !written.starts_with("//")
        && written.len() <= 1024
        && !written.chars().any(|c| c.is_control() || c == '\\'))
    .then(|| written.to_string())
}

/// The icon to take: a PNG picture, the largest that is no wider than 64 pixels, or
/// else the smallest there is.
fn icon(resource: &Element) -> Option<String> {
    let mut pictures: Vec<(u32, String)> = resource
        .all("Icons")
        .into_iter()
        .flat_map(|icons| icons.inside.iter())
        .filter(|icon| icon.attribute("FileType").is_some_and(|kind| kind.eq_ignore_ascii_case("png")))
        .filter_map(|icon| {
            let wide = icon.attribute("Dimensions").and_then(|size| size.split('x').next()?.parse().ok()).unwrap_or(32);
            Some((wide, path(icon.attribute("FileURL")?)?))
        })
        .collect();
    pictures.sort();
    let fitting = pictures.iter().rposition(|(wide, _)| *wide <= 64).unwrap_or(0);
    (!pictures.is_empty()).then(|| pictures.swap_remove(fitting).1)
}

/// How something a farm publishes is reached, as its file says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reached {
    /// The address to connect to: the farm's broker, or the machine itself.
    pub address: Option<String>,
    /// The collection the broker is asked for.
    pub collection: Option<String>,
    /// The program to start, by the name the farm knows it by.
    pub program: Option<String>,
}

/// Reads the file that says how something is reached: lines of a name, a kind and a value.
pub fn read_file(bytes: &[u8]) -> Reached {
    // such a file is written in two bytes a character, or in UTF-8
    let text = match bytes {
        [0xff, 0xfe, rest @ ..] => {
            String::from_utf16_lossy(&rest.as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).collect::<Vec<u16>>())
        }
        _ => String::from_utf8_lossy(bytes).trim_start_matches('\u{feff}').to_string(),
    };
    let mut reached = Reached::default();
    for line in text.lines() {
        let mut parts = line.splitn(3, ':');
        let (Some(name), Some(_kind), Some(value)) = (parts.next(), parts.next(), parts.next()) else { continue };
        let value = value.trim();
        if value.is_empty() || value.chars().count() > 253 || value.chars().any(char::is_control) {
            continue;
        }
        match name.trim().to_ascii_lowercase().as_str() {
            "full address" => reached.address = Some(value.to_ascii_lowercase()),
            // tsv://MS Terminal Services Plugin.1.<collection>
            "loadbalanceinfo" => {
                reached.collection =
                    value.rsplit('.').next().filter(|name| !name.is_empty() && value.starts_with("tsv://")).map(str::to_string)
            }
            "remoteapplicationprogram" => reached.program = Some(value.trim_start_matches('|').to_string()),
            _ => {}
        }
    }
    reached
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A feed as a farm's web access writes it.
    pub(crate) const FEED: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<ResourceCollection
  PubDate="2026-11-14T19:24:19.8928707Z"
  SchemaVersion="1.1" xmlns="http://schemas.microsoft.com/ts/2007/05/tswf">
  <Publisher
    LastUpdated="2026-11-14T18:06:02.9437861Z"
    Name="Work Resources"
    ID="broker.corp.example"
    Description="">
    <Resources>
      <Resource
        ID="ledger"
        Alias="ledger"
        Title="Ledger"
        LastUpdated="2026-11-01T08:00:00Z"
        Type="RemoteApp"
        ShowByDefault="True">
        <Icons>
          <IconRaw
            FileType="Ico"
            FileURL="/RDWeb/Feed/rdp/ledger-Finance-CmsRdsh.ico" />
          <Icon16
            Dimensions="16x16"
            FileType="Png"
            FileURL="/RDWeb/Feed/rdp/ledger-Finance-CmsRdsh_16x16.png" />
          <Icon32
            Dimensions="32x32"
            FileType="Png"
            FileURL="/RDWeb/Feed/rdp/ledger-Finance-CmsRdsh_32x32.png" />
          <Icon256
            Dimensions="256x256"
            FileType="Png"
            FileURL="/RDWeb/Feed/rdp/ledger-Finance-CmsRdsh_256x256.png" />
        </Icons>
        <FileExtensions />
        <HostingTerminalServers>
          <HostingTerminalServer>
            <ResourceFile
              FileExtension=".rdp"
              URL="/RDWeb/Feed/rdp/cpub-ledger-Finance-CmsRdsh.rdp" />
            <TerminalServerRef
              Ref="broker.corp.example" />
          </HostingTerminalServer>
        </HostingTerminalServers>
      </Resource>
      <Resource ID="pay" Alias="pay" Title="Pay &amp; Benefits" LastUpdated="2026-11-02T08:00:00Z" Type="RemoteApp">
        <Icons />
        <HostingTerminalServers><HostingTerminalServer><ResourceFile FileExtension=".rdp" URL="//elsewhere.example/x.rdp" /></HostingTerminalServer></HostingTerminalServers>
      </Resource>
      <Resource ID="desk" Alias="Desktops" Title="Desktop" LastUpdated="2026-11-03T08:00:00Z" Type="Desktop">
        <Icons><Icon32 Dimensions="32x32" FileType="Png" FileURL="/RDWeb/Feed/rdp/Desktops-CmsRdsh_32x32.png" /></Icons>
        <HostingTerminalServers><HostingTerminalServer><ResourceFile FileExtension=".rdp" URL="/RDWeb/Feed/rdp/cpub-Desktops-CmsRdsh.rdp" /></HostingTerminalServer></HostingTerminalServers>
      </Resource>
      <Resource ID="ledger2" Alias="ledger2" Title="LEDGER" Type="RemoteApp" />
    </Resources>
    <TerminalServers><TerminalServer ID="broker.corp.example" Name="broker.corp.example" LastUpdated="2026-11-14T18:06:02Z" /></TerminalServers>
  </Publisher>
</ResourceCollection>
"#;

    #[test]
    fn a_feed_lists_the_desktops_and_the_programs_with_their_icons_and_files() {
        let resources = read(FEED).unwrap();
        assert_eq!(resources.len(), 3, "the same name twice: the first one stands");
        assert_eq!(
            resources[0],
            Resource {
                title: "Ledger".to_string(),
                alias: "ledger".to_string(),
                desktop: false,
                changed: "2026-11-01T08:00:00Z".to_string(),
                // the largest picture that is no wider than 64 pixels
                icon: Some("/RDWeb/Feed/rdp/ledger-Finance-CmsRdsh_32x32.png".to_string()),
                file: Some("/RDWeb/Feed/rdp/cpub-ledger-Finance-CmsRdsh.rdp".to_string()),
            }
        );
        // a name is read as it is meant, and what leads elsewhere than the farm is left out
        assert_eq!((resources[1].title.as_str(), resources[1].icon.as_ref(), resources[1].file.as_ref()), ("Pay & Benefits", None, None));
        assert_eq!((resources[2].title.as_str(), resources[2].desktop, resources[2].alias.as_str()), ("Desktop", true, "Desktops"));
    }

    #[test]
    fn what_is_no_feed_is_refused() {
        assert_eq!(read("<html><body>sign in</body></html>"), Err("it is not a feed of what a farm publishes".to_string()));
        assert!(read("not xml").is_err());
        assert_eq!(
            read("<ResourceCollection><Resource Alias=\"a\"/></ResourceCollection>"),
            Err("it lists something without a name".to_string())
        );
        assert_eq!(read("<ResourceCollection/>"), Ok(Vec::new()));
    }

    #[test]
    fn a_file_says_where_to_connect_which_collection_and_which_program() {
        let file = "redirectclipboard:i:1\r\nfull address:s:BROKER.CORP.EXAMPLE\r\nloadbalanceinfo:s:tsv://MS Terminal Services Plugin.1.Finance\r\nremoteapplicationmode:i:1\r\nremoteapplicationprogram:s:||ledger\r\nalternate shell:s:||ledger\r\n";
        let expected = Reached {
            address: Some("broker.corp.example".to_string()),
            collection: Some("Finance".to_string()),
            program: Some("ledger".to_string()),
        };
        assert_eq!(read_file(file.as_bytes()), expected);
        // the same in two bytes a character, as such files are often written
        let wide: Vec<u8> = [0xff, 0xfe].into_iter().chain(file.encode_utf16().flat_map(u16::to_le_bytes)).collect();
        assert_eq!(read_file(&wide), expected);
        // a desktop names no program, and a file of nothing says nothing
        assert_eq!(read_file(b"full address:s:rdsh-01.corp.example\n").program, None);
        assert_eq!(read_file(b"\x00\x01garbage"), Reached::default());
    }
}
