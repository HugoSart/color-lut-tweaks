use std::process;

fn main() {
    if let Err(err) = color_lut_tweaks::cli::run(std::env::args().skip(1)) {
        eprintln!("error: {err}");
        eprintln!();
        color_lut_tweaks::cli::print_usage();
        process::exit(1);
    }
}
