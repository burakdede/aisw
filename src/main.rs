fn main() {
    if let Err(e) = aisw::run() {
        let exit_code = e
            .downcast_ref::<aisw::error::AiswError>()
            .map(|ae| ae.exit_code())
            .unwrap_or(aisw::error::EXIT_GENERAL_ERROR);
        aisw::output::print_error_chain(&e);
        std::process::exit(exit_code);
    }
}
