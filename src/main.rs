
// extern crate definitions
#[macro_use]
extern crate error_chain;
extern crate reqwest;
extern crate strum;
#[macro_use]
extern crate strum_macros;
extern crate tokio;

// internal mods
mod lol_api;
mod crawler;

use std::env;

fn usage(){
    println!("Usage: lol-match-crawler.exe <riot_api_key>")
}

async fn do_main() -> lol_api::Result<()> {

    // launch environment using api key
    let args : Vec<String> = env::args().collect();

    match args.get(1) {
        Some(_/*key*/) => {}, //ctx = lol_api::Context::new(&key),
        None => { usage(); return Err(lol_api::Error::from("missing command line argument.".to_string())) }
    }

    Ok(())
}

/// Workaround to integrate error-chain with async main function
/// in tokio. Pretty much just an expansion of the `quick_main!`
/// macro provided by error-chain
#[tokio::main]
async fn main() {
    if let Err(ref e) = do_main().await {
        use error_chain::ChainedError;
        use std::io::Write; // trait which holds `display_chain`
        let stderr = &mut ::std::io::stderr();
        let errmsg = "Error writing to stderr";

        writeln!(stderr, "{}", e.display_chain()).expect(errmsg);
        ::std::process::exit(1);
    }
}