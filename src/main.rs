mod app;
mod audio;
mod config;
mod cover;
mod library;
mod locale;
mod lyrics;
mod persist;
mod playlist_io;
mod state;
mod tray;
mod ui;
mod youtube;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const LICENSE: &str = include_str!("../LICENSE");

fn main() -> iced::Result {
    // Only the first argument matters: every option prints and exits.
    if let Some(arg) = std::env::args().nth(1) {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("lavanda {VERSION}");
                std::process::exit(0);
            }
            "--about" => {
                println!("{} {VERSION}", audio::mpris::APP_NAME);
                println!("Native Linux music player with synced lyrics, furigana and romaji,");
                println!("and YouTube downloads. A fork of lavanda (github.com/sheep-farm/lavanda).");
                println!();
                println!("Repository : https://github.com/CodingTort/vr_hv1-s-music-client");
                println!("License    : MIT");
                std::process::exit(0);
            }
            "--license" => {
                print!("{LICENSE}");
                std::process::exit(0);
            }
            "--help" | "-h" => {
                println!("{} {VERSION}", audio::mpris::APP_NAME);
                println!("Native Linux music player with synced lyrics.");
                println!();
                println!("USAGE:");
                println!("    lavanda [OPTIONS]");
                println!();
                println!("OPTIONS:");
                println!("    -h, --help       Print this help message");
                println!("    -V, --version    Print version");
                println!("        --about      About this program");
                println!("        --license    Print the MIT license");
                println!();
                println!("CONFIG:  ~/.config/lavanda/config.toml");
                println!("STATE:   ~/.config/lavanda/state.toml");
                std::process::exit(0);
            }
            unknown => {
                eprintln!("lavanda: unknown option '{unknown}'");
                eprintln!("Try 'lavanda --help' for usage.");
                std::process::exit(1);
            }
        }
    }

    config::load();
    locale::load();
    let cfg = config::get();
    ui::theme::reload_with_mode(&cfg.theme_mode, &cfg.theme_preset);
    ui::theme::set_corner_radius(cfg.corner_radius);
    persist::init();
    app::run()
}
