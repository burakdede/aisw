fn main() {
    if let Err(e) = aisw::run() {
        let exit_code = e
            .downcast_ref::<aisw::error::AiswError>()
            .map(|ae| ae.exit_code())
            .unwrap_or(aisw::error::EXIT_GENERAL_ERROR);
        if aisw::runtime::is_machine_mode() {
            aisw::machine::print_failure(None, &e, exit_code);
        } else {
            aisw::output::print_error_chain(&e);
        }
        std::process::exit(exit_code);
    }
}
