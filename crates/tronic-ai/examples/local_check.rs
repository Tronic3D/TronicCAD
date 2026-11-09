//! Checks the provider code against a real local model server.
//! Usage: `cargo run -p tronic-ai --example local_check -- <url> <model>`
#![allow(clippy::panic, clippy::print_stdout)]

use std::sync::atomic::AtomicBool;
use std::time::Instant;
use tronic_ai::*;

fn main() {
    let mut args = std::env::args().skip(1);
    let base_url = args.next().unwrap_or_else(|| panic!("url missing"));
    let model = args.next().unwrap_or_else(|| panic!("model missing"));
    let provider = HttpProvider::new(
        ProviderConfig {
            kind: ProviderKind::OpenAiCompatible,
            base_url,
            model,
            max_tokens: Some(40),
        },
        None,
    )
    .unwrap_or_else(|error| panic!("{error}"));
    println!("models: {:?}", provider.models());
    println!("loaded: {:?}", provider.loaded_models());
    let request = ChatRequest {
        system: "You answer briefly.".to_owned(),
        messages: vec![ChatMessage {
            role: Role::User,
            content: "Once upon a time".to_owned(),
        }],
    };
    let mut pieces = 0;
    let started = Instant::now();
    let outcome = provider
        .chat(&request, &AtomicBool::new(false), &mut |_| pieces += 1)
        .unwrap_or_else(|error| panic!("{error}"));
    println!(
        "pieces: {pieces}, stop: {:?}, tokens: {:?} in, {:?} out, {:.1} s",
        outcome.stop,
        outcome.input_tokens,
        outcome.output_tokens,
        started.elapsed().as_secs_f64()
    );
    println!("text: {}", outcome.text);
}
