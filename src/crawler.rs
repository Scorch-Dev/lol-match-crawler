
use crate::lol_api::Context;
use std::fs::File;

struct Crawler<'a> {
    context : &'a Context,
    file_out : File,
}

impl<'a> Crawler<'a> {

    #[allow(dead_code)]
    pub fn new(context : &'a Context) -> Result<Crawler<'a>, std::io::Error> {
        let file_out = File::create(format!("lol_data-{}", chrono::Utc::now().format("%F-%T")))?;
        Ok(Crawler {
            context : context,
            file_out : file_out,
        })
    }

    #[allow(dead_code)]
    pub fn start_crawl(&mut self) {
    }
}