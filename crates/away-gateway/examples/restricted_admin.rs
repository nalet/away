//! Explicit live probe: requires the normal Restricted Admin environment policy,
//! AWAY_TEST_SUBJECT, AWAY_TEST_TARGET and AWAY_TEST_FINGERPRINT. Opens one desktop,
//! verifies a nonblank frame and disconnects it. No credential values are printed.
use away_gateway::{
    machine::{reach_restricted_admin, Opened, Screen, Who},
    restricted_admin::RestrictedAdmin,
};
use std::{collections::BTreeSet, time::Duration};

#[tokio::main]
async fn main() {
    let policy = RestrictedAdmin::read(&|k| std::env::var(k).ok(), &|p| std::fs::read(p)).expect("valid policy").expect("enabled policy");
    let subject = std::env::var("AWAY_TEST_SUBJECT").expect("AWAY_TEST_SUBJECT");
    let target = std::env::var("AWAY_TEST_TARGET").expect("AWAY_TEST_TARGET");
    let pin = std::env::var("AWAY_TEST_FINGERPRINT").expect("AWAY_TEST_FINGERPRINT from a trusted host inventory");
    policy.account_for(&subject, &target, 3389).expect("allowed subject and target");
    let secured = reach_restricted_admin(&target, 3389).await.expect("Restricted Admin negotiation").secure().await.expect("TLS");
    assert_eq!(secured.identity().fingerprint_text(), pin, "RDP certificate must match the independently verified pin");
    println!("Restricted Admin and pinned TLS accepted");
    let ticket = policy.ticket(&subject, &target, 3389).await.expect("scoped delegated ticket");
    println!("Scoped ticket for {}@{} to {}", ticket.account.parts.join("/"), ticket.realm, ticket.service.parts.join("/"));
    let Opened::Desktop(mut desktop) =
        secured.sign_in(Who::RestrictedAdmin(ticket), Screen::of(1280, 720), 0x0807, None, None).await.expect("Restricted Admin sign-in")
    else {
        panic!("unexpected redirect")
    };
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            desktop.take().await.expect("desktop drawing");
            let picture = desktop.picture();
            if picture.width >= 640
                && picture.height >= 480
                && picture.pixels.as_chunks::<4>().0.iter().map(|p| [p[0], p[1], p[2]]).collect::<BTreeSet<_>>().len() > 8
            {
                return (picture.width, picture.height);
            }
            desktop.arrives().await.expect("desktop data");
        }
    })
    .await;
    desktop.leave().await;
    let size = result.expect("nonblank first desktop frame");
    println!("Nonblank {}x{} desktop; disconnected probe", size.0, size.1);
}
