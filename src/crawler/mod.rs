//! Encapsulates a "crawler" object
//! which uses a `lol_api::Context` to
//! crawl match histories and record relevent
//! match data to an output file.
//! 
//! For speed, multiple crawlers can share 
//! internal state via `clone()` so you can
//! run multiple crawlers in parallel while
//! not storing data redundantly.

mod errors;
pub use errors::*;

use crate::lol_api;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

/// The inner data of a single crawler which lives across
/// threads. Creating a new crawler instantiates
/// one inner data, and cloning the crawler reuses
/// the inner data for running multiple crawlers at once.
struct CrawlerInner {
    context : lol_api::Context,
    file_out : Mutex<File>,
    found_match_ids : Mutex<HashSet<i64>>,
}

/// A thin Arc wrapper which holds an Arc to the inner
/// crawler data, which can be shared across threads.
/// To reuse the same inner data (required to properly
/// have multiple crawlers write to the same output file)
/// create one crawler and clone it to share it's internal data
#[derive(Clone)]
pub struct Crawler {
    inner : Arc<CrawlerInner>,
}

impl Crawler {

    /// ctor consumes a context and moves it into an Arc
    /// inner struct as described in the documentation for the
    /// Crawler struct. This will also open an output
    /// file for writing in the current directory
    /// with the name "lol_data" followed by the timestamp
    /// 
    /// # Arguments
    /// 
    /// `context` - the lol api context to use for the crawler
    ///             The context is moved in
    /// 
    /// # Return
    /// 
    /// `Ok(Crawler)` if the crawler was constructed correctly.
    /// `Err(errors::Error)` if the construction failed (likely
    /// because the os couldn't open the output file for writing)
    pub async fn new(context : lol_api::Context) -> Result<Crawler> {
        let f_name = format!("./lol_data-{}", chrono::Utc::now().format("%F-%H-%M-%S"));
        let file_out = File::create(f_name).await?;
        Ok(Crawler {
            inner : Arc::new(CrawlerInner {
                context : context,
                file_out : Mutex::new(file_out),
                found_match_ids : Mutex::new(HashSet::new()),
            })
        })
    }

    /// Begins the crawl for match data. It takes
    /// the provided seed summoner name and gets the match history
    /// for that summoner. It then proceeds to crawl the match
    /// history for an unseen match, records the data, and moves
    /// restarts the match history crawl on a random summoner from the
    /// newly recorded match.
    /// 
    /// # Arguments
    /// 
    /// * `seed_summoner_name` - the summoner name to use for getting the first
    ///     match history to crawl.
    /// * `num_steps` - The number of matches to fetch in total. If the result is
    ///     an error, then up to this many matches may still have been recorded in the
    ///     output file.
    /// 
    /// # Return
    /// 
    /// `Ok(())` if `num_steps` number of matches were succesfully recorded to the file
    /// `Err(errors::Error)` if less than the `num_steps` number of matches output
    /// 
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

    /// Consolidates the steps of both crawling a match history
    /// for an unseen match and reserving the match id for future
    /// use by marking it as "seen". Useful to avoid needing
    /// to lock the entire seen pool while we copy data from
    /// the match to the output and pick out a random summoner
    /// to source our next match history from.
    /// 
    /// # Arguments
    /// 
    /// * `inner` - the crawler's inner data to avoid tying
    ///     this to an instance of the crawler so it can run
    ///     on another thread
    /// * `matchlist_dto` - the previously-fetched match history
    ///     for a summoner
    /// 
    /// # Return
    /// 
    /// Some(i64) containing the found match id
    /// None if the match history contains no unseen matches
    /// 
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

    /// Takes a match and selects one of the match participants at random
    /// and gives us back their encrypted account id
    /// 
    /// # Arguments
    /// 
    /// * `match_dto` - a reference to the match dto to select a summoner from
    /// 
    /// # Return
    /// 
    /// A string slice referring to the encrypted account id of the random
    /// participant inside the provided `match_dto`
    fn random_account_id<'a>(match_dto : &'a lol_api::MatchDto) -> &'a str {

        let participant_idx = rand::random::<usize>() % match_dto.participant_identities.len();
        &match_dto.participant_identities
                 .get(participant_idx).unwrap()
                 .player.account_id
    }

    /// Runs the algorithm to crawl and do the heavy lifting.
    /// 
    /// 1. queries the lol api for details on a given match
    /// 1. writes the match data to the output file
    /// 1. Takes a random account and queries the lol api for their match
    ///    history
    /// 1. reserves a new match id from that match history
    /// 1. go back to step 1. and repeat until the desired
    ///    number of matches are feched
    /// 
    /// # Arguments
    /// 
    /// * `inner` - the crawler's inner data to avoid tying
    ///     this to an instance of the crawler so it can run
    ///     on another thread
    /// * `match_count` - how many matches should be fetched
    /// * `seed_match_id` - the first match to record
    /// 
    /// # Return
    /// 
    /// * `Ok(())` if `num_matches` was found
    /// * `Err(lol_api::Error)` if less than num matches were found
    ///   (often because the lol_api couldn't be accessed or the crawler)
    ///   reached a "dead end" in the course of the crawl (e.g. edge case 
    ///   where summoner only has one match in their match history).
    async fn do_crawl_work(
        inner : Arc<CrawlerInner>,
        match_count : usize, seed_match_id : i64) -> Result<()>{

        let mut match_id = seed_match_id;
        for i in 0..match_count {

            // get match, record data, and add to 'seen' set
            let match_dto = inner.context.query_match_v4_match_by_id(lol_api::Region::Na1, match_id, 3).await?;
            Self::write_match_to_file(inner.clone(), &match_dto).await?;

            // get next match from that participants match history
            if i != (match_count - 1) {
                let account_id = Self::random_account_id(&match_dto);
                let matchlist_dto = inner.context.query_match_v4_matchlist_by_account(lol_api::Region::Na1, account_id, 3).await?;
                match_id = Self::reserve_new_match_id(inner.clone(), &matchlist_dto).await.unwrap();
            }
        }

        Ok(())
    }

    /// Selects important data from a match data object
    /// and writes it asynchrnously to the output file.
    /// 
    /// # Arguments
    /// 
    /// * `inner` - the crawler's inner data to avoid tying
    ///     this to an instance of the crawler so it can run
    ///     on another thread
    /// * `match_dto` - the match to cherry-pick the data from
    /// 
    /// # Return
    /// 
    /// `Ok(())` if the file was written to sucesfully
    /// `Err(lol_api::Error)` if the file could not be written to
    ///     (in which case the error wraps an io::Error)
    /// 
    async fn write_match_to_file(inner : Arc<CrawlerInner>, match_dto : &lol_api::MatchDto) -> Result<()> {

        let mut line_items : Vec<String> = Vec::new();

        //participant stats
        for participant in &match_dto.participants {

            // champ
            line_items.push(participant.champion_id.to_string());

            //spells
            line_items.push(participant.spell1_id.to_string());
            line_items.push(participant.spell2_id.to_string());

            //masteries
            for mastery in &participant.masteries {
                line_items.push(mastery.mastery_id.to_string());
                line_items.push(mastery.rank.to_string());
            }

            //runes
            for rune in &participant.runes {
                line_items.push(rune.rune_id.to_string());
                line_items.push(rune.rank.to_string());
            }

            // highest achieved season tier
            line_items.push(participant.highest_achieved_season_tier.clone());

            //role and lane
            line_items.push(participant.timeline.lane.clone());
            line_items.push(participant.timeline.role.clone());
        }

        // push the line to the output
        let mut line = line_items.join(",");
        line.push('\n');
        
        let mut file_lock = inner.file_out.lock().await;
        file_lock.write_all(&line.into_bytes()).await?;

        Ok(())
    }
}


#[cfg(test)]
mod tests {

    use super::Crawler;
    use crate::lol_api::Context;
    use tokio::runtime::Runtime;

    /// ctor test for the constructor. 
    /// Makes sure we can do things
    /// like construct the output file 
    /// and keep track of the internal state without exploding
    #[test]
    fn test_ctor() {
        let mut rt = Runtime::new().expect("couldn't instantiate tokio runtime!");
        let key = crate::util::get_key();
        let ctx = Context::new(&key);

        rt.block_on(async move {
            let crawler = Crawler::new(ctx).await;
            assert!(crawler.is_ok());
        });
    }
}