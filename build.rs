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
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let man_dir = out_dir.join("man");
    let out_completions_dir = out_dir.join("completions");
    std::fs::create_dir_all(&man_dir).expect("could not create OUT_DIR man directory");
    std::fs::create_dir_all(&out_completions_dir)
        .expect("could not create OUT_DIR completions directory");

    use clap::CommandFactory;
    let cmd = cli::Cli::command();
    let man = clap_mangen::Man::new(cmd);
    let mut buffer = Vec::new();
    man.render(&mut buffer).expect("could not render man page");
    std::fs::write(man_dir.join("aisw.1"), &buffer).expect("could not write man/aisw.1");

    generate_completions(&out_completions_dir);

    println!("cargo:rerun-if-changed=src/cli.rs");
    println!("cargo:rerun-if-changed=src/types.rs");
}

fn generate_completions(dir: &std::path::Path) {
    use clap::CommandFactory;
    use clap_complete::{
        generate_to,
        shells::{Bash, Fish, Zsh},
    };

    generate_to(Bash, &mut cli::Cli::command(), "aisw", dir)
        .expect("could not generate bash completions");
    generate_to(Zsh, &mut cli::Cli::command(), "aisw", dir)
        .expect("could not generate zsh completions");
    generate_to(Fish, &mut cli::Cli::command(), "aisw", dir)
        .expect("could not generate fish completions");
}
