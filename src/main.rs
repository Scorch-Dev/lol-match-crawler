
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
mod util;

use std::env;

fn usage(){
    println!("Usage: lol-match-crawler.exe")
}

error_chain!{
    links {
        Crawler(crate::crawler::Error, crate::crawler::ErrorKind);
    }
}

async fn do_main() -> Result<()> {

    // ensure proper number of args
    let args : Vec<String> = env::args().collect();
    if args.len() != 1 {
        usage();
        return Err(Error::from(format!("Invalid number of command line arguments. Expected 0, got {}", args.len())));
    }

    // get api key from key.txt
    let key = util::get_key();

    //instance ctx
    let ctx = lol_api::Context::new(&key);

    // run the crawlers in a join
    let c1 = crawler::Crawler::new(ctx).await.expect("unable to instance riot api crawler!");
    let c2 = c1.clone();
    let c3 = c1.clone();
    let c4 = c1.clone();
    let r = tokio::join!(
        c1.start_crawl("hi", 10),
        c2.start_crawl("hi", 10),
        c3.start_crawl("hi", 10),
        c4.start_crawl("hi", 10),
    );

    r.0?;
    r.1?;
    r.2.expect("crawler 2 died");
    r.3.expect("crawler 3 died");

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