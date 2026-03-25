fn main() {
    if let Err(e) = aisw::run() {
        let chain: Vec<String> = e.chain().map(|c| c.to_string()).collect();
        if let Some((first, rest)) = chain.split_first() {
            eprintln!("Error: {}", first);
            for msg in rest {
                eprintln!("  {}", msg);
            }
        }
        std::process::exit(1);
    }
}
