use std::path::PathBuf;

// Include the CLI definition so clap_mangen can generate the man page without
// depending on the library crate (which would be a circular build dependency).
mod types {
    #![allow(dead_code)]
    include!("src/types.rs");
}
mod cli {
    // `crate::types::Tool` in the included file resolves to the `types` module above.
    include!("src/cli.rs");
}

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let man_dir = manifest_dir.join("man");
    std::fs::create_dir_all(&man_dir).expect("could not create man/ directory");

    use clap::CommandFactory;
    let cmd = cli::Cli::command();
    let man = clap_mangen::Man::new(cmd);
    let mut buffer = Vec::new();
    man.render(&mut buffer).expect("could not render man page");
    std::fs::write(man_dir.join("aisw.1"), &buffer).expect("could not write man/aisw.1");

    println!("cargo:rerun-if-changed=src/cli.rs");
    println!("cargo:rerun-if-changed=src/types.rs");
}
