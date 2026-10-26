//! Prints the snapshot built from a recorded probe, for demos and UI work:
//! `cargo run -p sesame-discovery --example fixture_snapshot`

fn main() {
    let mut snap = sesame_discovery::from_probe(include_str!("../fixtures/frankenphp-octane.probe"));
    sesame_discovery::apply_details(&mut snap, include_str!("../fixtures/frankenphp-octane.details"));
    sesame_discovery::link(&mut snap);
    println!("{}", serde_json::to_string_pretty(&snap).unwrap());
}
