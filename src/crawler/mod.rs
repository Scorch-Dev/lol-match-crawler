
mod errors;
pub use errors::*;

use crate::lol_api;
use std::collections::HashSet;
use std::fs::File;
use std::sync::Arc;
use tokio::sync::Mutex;

const NUM_CONCURRENT_CRAWLS : usize = 4;

struct Crawler {
    context : Arc<lol_api::Context>,
    file_out : Arc<File>,
    found_match_ids : Arc<Mutex<HashSet<i64>>>,
}

impl Crawler {

    #[allow(dead_code)]
    pub fn new(context : lol_api::Context) -> Result<Crawler> {
        let file_out = File::create(format!("lol_data-{}", chrono::Utc::now().format("%F-%T")))?;
        Ok(Crawler {
            context : Arc::new(context),
            file_out : Arc::new(file_out),
            found_match_ids : Arc::new(Mutex::new(HashSet::new())),
        })
    }

    #[allow(dead_code)]
    pub async fn start_crawl(&self, num_matches : usize, seed_summoner_name : &str) -> Result<()> {

        let seed_account_id = self.context
                                .query_summoner_v4_by_summoner_name(lol_api::Region::Na1, seed_summoner_name, 3).await
                                .chain_err(|| "Unable to get seed summoner id.")?
                                .account_id;


        //first gather some match ids
        let mut seed_match_ids : Vec<i64> = Vec::new();
        self.get_match_ids(&seed_account_id, NUM_CONCURRENT_CRAWLS, &mut seed_match_ids).await?;

        // divide work among crawlers and launch them
        let mut join_handles : Vec<tokio::task::JoinHandle<()>>= Vec::new();
        let per_crawler_count = ((seed_match_ids.len() as f64) / (NUM_CONCURRENT_CRAWLS as f64)).ceil() as usize;

        for (idx, match_id) in seed_match_ids.iter().enumerate() {

            // make sure we don't over/undershoot requested number match data items
            let until_finished = seed_match_ids.len() - (per_crawler_count * idx) + 1;

            // copy all so the refs live long enough
            let crawl_count = std::cmp::min(until_finished, per_crawler_count);
            let id = *match_id;
            let context = self.context.clone();
            let file_out = self.file_out.clone();
            let found_match_ids = self.found_match_ids.clone();

            let handle = tokio::spawn( async move {
                Self::do_crawl_work(context, file_out, found_match_ids, crawl_count, id).await 
            });
            join_handles.push(handle);
        }

        // await our crawlers and return
        for handle in join_handles.drain(..) {
            handle.await?;
        }
        
        Ok(())
    }

    async fn get_match_ids(&self, account_id : &str, num_matches : usize, match_ids_out : &mut Vec<i64>) -> Result<usize> {

        let matchlist_dto = self.context
                            .query_match_v4_matchlist_by_account(lol_api::Region::Na1, account_id, 3).await
                            .chain_err(|| "Unable to get seed summoner matchlist")?;

        let found_match_ids_lock = self.found_match_ids.lock().await;
        let mut num_matches_found = 0;
        for match_ref_dto in (&matchlist_dto.matches).iter() {

            // only add a match if we've never seen it
            if !found_match_ids_lock.contains(&match_ref_dto.game_id) {
                match_ids_out.push(match_ref_dto.game_id);
                num_matches_found += 1;
            }

            //break early when we got what we came for
            if num_matches_found == num_matches {
                break;
            }
        }

        Ok(num_matches_found)
    }

    async fn do_crawl_work(
        context : Arc<lol_api::Context>, file_out : Arc<File>, found_match_ids : Arc<Mutex<HashSet<i64>>>,
        match_count : usize, seed_match_id : i64) {

        let found_count = 0;
        while found_count < match_count {

            // get match
            let match_dto = context.query_match_v4_match_by_id(lol_api::Region::Na1, seed_match_id, 3).await;
            //if match_dto.is_err() { return; }

            // select random participant
            //let participant_ids = 

            //
        }
    }
}