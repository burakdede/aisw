fn main() {
    if let Err(e) = aisw::run() {
        aisw::output::print_error_chain(&e);
        std::process::exit(1);
    }
}
