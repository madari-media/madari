//! Prints the TV web-settings OpenAPI 3.1 document for client code generation.
//!
//! ```bash
//! cargo run -p madari-tv --features openapi --example web_openapi > crates/madari-tv/web/openapi.json
//! ```
fn main() {
    let document = madari_tv::openapi_document();
    println!(
        "{}",
        document
            .to_pretty_json()
            .expect("OpenAPI document is serializable")
    );
}
