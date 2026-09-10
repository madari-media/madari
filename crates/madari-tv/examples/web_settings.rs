//! Local development server. Uses a separate, explicitly supplied data directory.
use madari_tv::Bridge;
fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("pass an isolated data directory");
    let bridge = Bridge::open(path.into()).expect("open development profile database");
    let status = bridge
        .call("web_start", serde_json::json!({}))
        .expect("start server");
    println!(
        "Open http://127.0.0.1:11471 · pairing code {}",
        status["code"].as_str().unwrap()
    );
    loop {
        std::thread::park();
    }
}
