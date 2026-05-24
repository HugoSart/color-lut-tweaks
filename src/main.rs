use std::process;

fn main() {
    color_lut_tweaks::logging::init();

    if let Err(err) = color_lut_tweaks::cli::run(std::env::args().skip(1)) {
        color_lut_tweaks::logging::error(format!("command failed: {err}"));
        eprintln!("error: {err}");
        eprintln!();
        color_lut_tweaks::cli::print_usage();
        process::exit(1);
    }
}
