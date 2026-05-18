use std::process;

fn main() {
    if let Err(err) = hdr_tweaks::cli::run(std::env::args().skip(1)) {
        eprintln!("error: {err}");
        eprintln!();
        hdr_tweaks::cli::print_usage();
        process::exit(1);
    }
}
