
mod errors;
pub use errors::*;

use crate::lol_api;
use std::collections::HashSet;
use std::fs::File;
use std::sync::Arc;
use tokio::sync::Mutex;

struct CrawlerInner {
    context : lol_api::Context,
    file_out : File,
    found_match_ids : Mutex<HashSet<i64>>,
}

struct Crawler {
    inner : Arc<CrawlerInner>,
}

impl Crawler {

    #[allow(dead_code)]
    pub fn new(context : lol_api::Context) -> Result<Crawler> {
        let file_out = File::create(format!("lol_data-{}", chrono::Utc::now().format("%F-%T")))?;
        Ok(Crawler {
            inner : Arc::new(CrawlerInner {
                context : context,
                file_out : file_out,
                found_match_ids : Mutex::new(HashSet::new()),
            })
        })
    }

    #[allow(dead_code)]
    pub async fn start_crawl(&self, seed_summoner_name : &str, num_steps : usize) -> Result<()> {

        let seed_account_id = self.inner.context
                                .query_summoner_v4_by_summoner_name(lol_api::Region::Na1, seed_summoner_name, 3).await
                                .chain_err(|| "Unable to get seed summoner id.")?
                                .account_id;

        // first get an unkown seed match id
        let matchlist_dto = self.inner.context.query_match_v4_matchlist_by_account(lol_api::Region::Na1, &seed_account_id, 3).await?;
        let seed_match_id = Self::reserve_new_match_id(self.inner.clone(), &matchlist_dto).await.unwrap();

        Self::do_crawl_work(self.inner.clone(), num_steps, seed_match_id).await
    }

    async fn reserve_new_match_id(inner : Arc<CrawlerInner>, matchlist_dto : &lol_api::MatchlistDto) -> Option<i64> {

        let mut found_match_ids = inner.found_match_ids.lock().await;
        let mut unkown_match_refs = matchlist_dto.matches.iter().skip_while(|x| found_match_ids.contains(&x.game_id));

        if let Some(first_unkown) = unkown_match_refs.next() {
            found_match_ids.insert(first_unkown.game_id);
            Some(first_unkown.game_id)
        }
        else {
            None
        }
    }

    fn random_account_id<'a>(match_dto : &'a lol_api::MatchDto) -> &'a str {

        let participant_idx = rand::random::<usize>() % match_dto.participant_identities.len();
        &match_dto.participant_identities
                 .get(participant_idx).unwrap()
                 .player.account_id
    }

    async fn do_crawl_work(
        inner : Arc<CrawlerInner>,
        match_count : usize, seed_match_id : i64) -> Result<()>{

        let mut match_id = seed_match_id;
        for i in 0..match_count {

            // get match, record data, and add to 'seen' set
            let match_dto = inner.context.query_match_v4_match_by_id(lol_api::Region::Na1, match_id, 3).await?;
            //TODO: save match data to file

            // get next match from that participants match history
            if i != (match_count - 1) {
                let account_id = Self::random_account_id(&match_dto);
                let matchlist_dto = inner.context.query_match_v4_matchlist_by_account(lol_api::Region::Na1, account_id, 3).await?;
                match_id = Self::reserve_new_match_id(inner.clone(), &matchlist_dto).await.unwrap();
            }
        }

        Ok(())
    }
}