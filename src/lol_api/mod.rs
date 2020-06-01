
// external uses
use reqwest::blocking::{Client, Response};
use reqwest::StatusCode;
use std::collections::HashMap;
use std::time::{Instant, Duration};
use chrono::DateTime;

// my mods
mod services;
mod errors;

pub use errors::*;
use services::summoner_v4;

#[derive(Debug)]
enum EndpointStatus {
    Unkown,                      // Used at initialization mostly
    Normal,                      // Go ahead and request at will
    RateLimited(RateLimitState), // estimated ms until we should retry
}

#[derive(Debug)]
struct RateLimitState {
    start : Instant,
    duration : Duration,
}

impl RateLimitState {
    fn new( duration_ms : u64)->RateLimitState {
        RateLimitState {
            start : Instant::now(),
            duration : Duration::from_millis(duration_ms),
        }
    }

    fn is_expired(&self) -> bool {
        let since_started = Instant::now().duration_since(self.start);
        match since_started.checked_sub(self.duration) {
            Some(_) => false,
            None => true,
        }
    }
}

#[derive(Debug)]
struct RateLimitBucket {
    count : u64,           // count so far
    max_count : u64,       // max before rate limiting
    start_timestamp : i64, // estimate of the start time based on last rollover in ms
}

#[derive(Debug)]
struct EndpointState {
    status : EndpointStatus,                            // state of this level
    rate_limit_buckets : HashMap<u64, RateLimitBucket>,             // map bucket duration to limit
    substates : Option<HashMap<u32, EndpointState>>,    // state of levels below if there are any (e.g. service below platform, method below service)
    last_update_timestamp_ms : i64,
}

impl EndpointState {
    fn new()->EndpointState {
        EndpointState {
            status : EndpointStatus::Unkown,
            rate_limit_buckets : HashMap::new(),
            substates : None,
            last_update_timestamp_ms : 0i64,
        }
    }

    fn set_buckets_from_headers(&mut self, limits_str : &str, counts_str :  &str) {

        let limit_strs = limits_str.split(",");
        let count_strs = counts_str.split(",");

        // first just update rate limits
        for limit_str in limit_strs {
            let limit = limit_str.split(":").nth(0).unwrap().parse::<u64>().unwrap();
            let bucket_size = limit_str.split(":").nth(1).unwrap().parse::<u64>().unwrap();
            let mut bucket = self.rate_limit_buckets.entry(bucket_size)
                .or_insert(RateLimitBucket {
                    count : 0,
                    max_count : 0,
                    start_timestamp : chrono::Utc::now().timestamp_millis(),
                });
            bucket.max_count = limit;
        }

        // set counts for existing buckets... They better exist by now
        for count_str in count_strs {
            let count = count_str.split(":").nth(0).unwrap().parse::<u64>().unwrap();
            let bucket_size = count_str.split(":").nth(1).unwrap().parse::<u64>().unwrap();
            let mut bucket = self.rate_limit_buckets.get_mut(&bucket_size).unwrap();
            bucket.count = count;
        }
    }
}

#[derive(Debug)]
pub struct Context {
    platform_states : HashMap<Region, EndpointState>,
    api_key : String,
    client : Client
}

// used to identify region
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum Region {
    Na1 = 0,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum Service {
    SummonerV4 = 0,
}

impl Context {

    pub fn new(api_key : &str) -> Context {
        Context{
            platform_states : HashMap::new(),
            api_key : api_key.to_string(),
            client : Client::new(),
        }
    }

    /** SUMMONER V4 METHODS */
    pub fn query_summoner_v4_by_summoner_name(
        &mut self, region : Region, summoner_name : &str)->Result<summoner_v4::SummonerDto>{

        let uri = Self::region_uri(region) + &summoner_v4::by_name_uri(summoner_name);
        let response = self.send_query(&uri, region, Service::SummonerV4, summoner_v4::Method::ByName as u32)?;
        let data = response.json::<summoner_v4::SummonerDto>()?;
        Ok(data)
    }

    pub fn query_summoner_v4_by_account(
        &mut self, region : Region, encrypted_account_id : &str)->Result<summoner_v4::SummonerDto> {

        let uri = Self::region_uri(region) + &summoner_v4::by_account_uri(encrypted_account_id);
        let response = self.send_query(&uri, region, Service::SummonerV4, summoner_v4::Method::ByAccount as u32)?;
        let data = response.json::<summoner_v4::SummonerDto>()?;
        Ok(data)
    }

    /** WORKHORSE QUERY METHOD */
    fn send_query(&mut self, uri : &str, platform : Region, service : Service, method_id : u32)->Result<Response> {

        // Get endpoint state or make if it doesn't exist. Then update and validate prior to query
        let platform_state = self.platform_states.entry(platform).or_insert(EndpointState::new());
        Self::pre_query_update_endpoint_state(platform_state);
        Self::pre_query_validate_endpoint_state(platform_state)?;

        let service_state = platform_state.substates.get_or_insert(HashMap::new())
                            .entry(service as u32).or_insert(EndpointState::new());
        Self::pre_query_update_endpoint_state(service_state);
        Self::pre_query_validate_endpoint_state(service_state)?;

        let method_state = service_state.substates.get_or_insert(HashMap::new())
                            .entry(method_id).or_insert(EndpointState::new());
        Self::pre_query_update_endpoint_state(method_state);
        Self::pre_query_validate_endpoint_state(method_state)?;

        // Send query here
        let response = self.client.get(uri)
            .header("X-Riot-Token", &self.api_key)
            .send()?;

        // Handle response here and update internal state
        match response.status() {

            // 200 ok - update internal state with successful header
            StatusCode::OK => {

                let date_str = response.headers().get("Date").unwrap().to_str().unwrap();
                let timestamp_ms = DateTime::parse_from_rfc2822(date_str).unwrap().timestamp_millis();

                let platform_state = self.platform_states.get_mut(&platform).unwrap();
                if timestamp_ms >= platform_state.last_update_timestamp_ms {
                    let rate_limits = response.headers().get("X-App-Rate-Limit").unwrap().to_str().unwrap();
                    let rate_limit_counts = response.headers().get("X-App-Rate-Limit-Count").unwrap().to_str().unwrap();
                    platform_state.set_buckets_from_headers(rate_limits, rate_limit_counts);
                }
                let service_state = platform_state.substates.as_mut().unwrap().get_mut(&(service as u32)).unwrap();
                let method_state = service_state.substates.as_mut().unwrap().get_mut(&method_id).unwrap();
                if timestamp_ms >= method_state.last_update_timestamp_ms {
                    let rate_limits = response.headers().get("X-Method-Rate-Limit").unwrap().to_str().unwrap();
                    let rate_limit_counts = response.headers().get("X-Method-Rate-Limit-Count").unwrap().to_str().unwrap();
                    method_state.set_buckets_from_headers(rate_limits, rate_limit_counts);
                }

                println!("{:?}", &self.platform_states);

                Ok(response)
            },

            // 429 rate limited - best guess the cooldown and set state to rate limited
            StatusCode::TOO_MANY_REQUESTS =>
                Err( Error::from( ErrorKind::from(response) ) ),

            // anything else - error out
            _ => {
                response.error_for_status()?;
                Err(Error::from(ErrorKind::from(""))) //should never get here
            },
        }
    }

    fn pre_query_update_endpoint_state(state : &mut EndpointState) {
        match &state.status {
            EndpointStatus::RateLimited(rl_state) if rl_state.is_expired() => {
                state.status = EndpointStatus::Unkown; //just because we expired, doesn't guarentee normal, the cooldown was a guess
            },
            _ => {}
        }
    }

    fn pre_query_validate_endpoint_state(state : &mut EndpointState)->Result<()> {
        match &state.status {
            EndpointStatus::Normal | EndpointStatus::Unkown => Ok(()),
            s => Err(Error::from(ErrorKind::from(format!("{:?}", s)))),
        }
    }

            // Errors that may be recoverable with naive retry
            // Errors that may be recoverable with backing off then retry
            // Non-recoverable Errors (usually programmer error)

    fn region_uri(region : Region)->String {
        format!("https://{:?}.api.riotgames.com", region)
    }
}