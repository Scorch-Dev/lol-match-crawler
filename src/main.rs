
extern crate reqwest;
#[macro_use]
extern crate error_chain;
extern crate strum;
#[macro_use]
extern crate strum_macros;

extern crate tokio;

mod lol_api;
mod crawler;
use std::env;

fn usage(){
    println!("Usage: lol-match-crawler.exe <riot_api_key>")
}

async fn do_main() -> lol_api::Result<()> {

    // launch environment using api key
    let args : Vec<String> = env::args().collect();
    let ctx;

    match args.get(1) {
        Some(key) => ctx = lol_api::Context::new(&key),
        None => { usage(); return Err(lol_api::Error::from("missing command line argument.".to_string())) }
    }

    //let account_id = ctx.try_query_summoner_v4_by_summoner_name(lol_api::Region::Na1, "hi").await?.account_id;
    for _ in 0..90 {

        /*
        let (dto_one, dto_two) = tokio::join!(
            ctx.query_summoner_v4_by_summoner_name(lol_api::Region::Na1, "hi", 3),
            ctx.query_summoner_v4_by_account(lol_api::Region::Na1, &account_id, 3));

        assert_eq!(dto_one?.account_id, dto_two?.account_id);
        */
        let dto = ctx.query_summoner_v4_by_summoner_name(lol_api::Region::Na1, "hi", 3).await;
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