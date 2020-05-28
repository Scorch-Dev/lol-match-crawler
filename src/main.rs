
extern crate reqwest;
#[macro_use]
extern crate error_chain;

mod lol_api;
use std::env;

fn usage(){
    println!("Usage: lol-match-crawler.exe <riot_api_key>")
}

fn do_main() -> lol_api::Result<()> {

    // launch environment using api key
    let args : Vec<String> = env::args().collect();
    let mut ctx;

    match args.get(1) {
        Some(key) => ctx = lol_api::Context::new(&key),
        None => { usage(); return Err(lol_api::Error::from("".to_string())) }
    }

    let dto = ctx.query_summoner_v4_by_summoner_name(lol_api::Region::Na1, "hi")?;
    println!("{:?}", dto);
    Ok(())
}

quick_main!(do_main);