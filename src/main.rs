mod api;
mod cli;
mod config;
mod context;
mod gemini;
mod groq;
mod openrouter;
mod runtime;
mod util;

pub fn main() {
    cli::exec();
}
