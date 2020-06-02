
// external uses
use chrono::DateTime;
use reqwest::blocking::{Client, Response};
use reqwest::StatusCode;
use std::collections::HashMap;
use std::time::{Instant, Duration};

// my mods
mod services;
mod errors;

pub use errors::*;
use services::summoner_v4;

#[derive(Debug)]
enum EndpointStatus {
    Unkown,                      // Used at initialization mostly
    Normal,                      // Go ahead and request at will
    Cooldown(CooldownState),     // The instant we started cooldown and the duration
    JustOffCooldown(Duration),   // State is unkown but we just got off a cooldown of the given duration
}

#[derive(Debug)]
struct CooldownState {
    start : Instant,
    duration : Duration,
}

impl CooldownState {
    fn new(duration : Duration)->CooldownState {
        CooldownState {
            start : Instant::now(),
            duration : duration,
        }
    }

    fn is_expired(&self) -> bool {
        let since_started = Instant::now().duration_since(self.start);
        match since_started.checked_sub(self.duration) {
            Some(_) => true,
            None => false,
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
struct Endpoint {
    status : EndpointStatus,                              // state of this level
    rate_limit_buckets : HashMap<u64, RateLimitBucket>, // map bucket duration to limit
    last_update_timestamp_ms : i64,
}

impl Endpoint {
    fn new()->Endpoint {
        Endpoint {
            status : EndpointStatus::Unkown,
            rate_limit_buckets : HashMap::new(),
            last_update_timestamp_ms : 0i64,
        }
    }

    fn set_buckets_from_headers(&mut self, limits_str : &str, counts_str :  &str, timestamp : i64) {

        let limit_strs = limits_str.split(",");
        let count_strs = counts_str.split(",");

        // first just update rate limits
        self.rate_limit_buckets.clear(); // in the future, only update when required
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
            let bucket = self.rate_limit_buckets.get_mut(&bucket_size).unwrap();

            if bucket.count > count { //detect rollover
                bucket.start_timestamp = timestamp;
            }
            bucket.count = count;
        }
    }

    fn should_cooldown(&self) -> Option<CooldownState> {
        for (bucket_size, bucket) in self.rate_limit_buckets.iter() {
            if bucket.count == bucket.max_count {
                return Some(CooldownState::new(Duration::from_secs(*bucket_size)));
            }
        }

        None
    }
}

#[derive(Debug)]
pub struct Context {
    endpoints : HashMap<usize, Endpoint>,
    api_key : String,
    client : Client
}

// used to identify region
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, EnumIter, EnumCount)]
pub enum Region {
    Na1 = 0,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, EnumIter, EnumCount)]
pub enum Service {
    SummonerV4 = 0,
}

const MAX_METHODS_PER_SERVICE : usize = 128; //need this since each service has its own methods

fn region_id_to_endpoint_id(region : Region) -> usize {
    region as usize
}

fn service_id_to_endpoint_id(service : Service) -> usize {
    REGION_COUNT + (service as usize)
}

fn method_id_to_endpoint_id(service : Service, method_id : u32) -> usize {
    REGION_COUNT + SERVICE_COUNT + ((service as usize) * MAX_METHODS_PER_SERVICE) + (method_id as usize)
}

impl Context {

    pub fn new(api_key : &str) -> Context {
        Context{
            endpoints : HashMap::new(),
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

    /// The workhorse method for synhrnous querying. We check internal state
    /// make sure the query is safe to execute (e.g. the endpoint isn't on cooldown and we can send),
    /// sends the request, blocks, caches rate-limiting related information,
    /// then returns the server response. If anything happens along the way or the server responds with
    /// anything but 200 - OK we return the error.
    fn send_query(&mut self, uri : &str, region : Region, service : Service, method_id : u32)->Result<Response> {

        self.prepare_to_query(region, service, method_id)?;

        let response = self.client.get(uri)
            .header("X-Riot-Token", &self.api_key)
            .send()?;

        self.handle_response(&response, region, service, method_id);

        match response.error_for_status() {
            Err(e) => Err(Error::from(e)),
            Ok(r) => Ok(r),
        }
    }

    fn handle_response(
        &mut self, response : &Response, region : Region, service : Service, method_id : u32) {
        
        match response.status() {

            StatusCode::OK =>
                self.handle_response_200(&response, region, service, method_id),

            StatusCode::TOO_MANY_REQUESTS =>
                self.handle_response_429(&response, region, service, method_id),

            _ => { }
        }
    }

    fn handle_response_200(
        &mut self, response : &Response, region : Region, service : Service, method_id : u32) {

        self.cache_rate_limits(response, region, service, method_id);
        self.handle_status_transitions(response, region, service, method_id,
            &Self::handle_response_200_status_transitions);
    }

    fn handle_response_429(
        &mut self, response : &Response, region : Region, service : Service, method_id : u32) {

        self.handle_status_transitions(
            response, region, service, method_id, 
            &Self::handle_response_429_status_transitions);
    }

    /// Helper method to avoid retyping the same thing over and over. Takes a state transition function
    /// and applies it to each of the endpoitns specified by region, service, and method. The
    /// transition function uses the result and the current status of any given endpoint to alter the endpoints
    /// current status.
    /// 
    fn handle_status_transitions(&mut self, response : &Response, region : Region, service : Service, method_id : u32, 
        transition_func : &dyn Fn(&Response, &mut Endpoint)) {
        
        {
            let region_ep  = self.endpoints.get_mut(&region_id_to_endpoint_id(region)).unwrap();
            transition_func(&response, region_ep);
        }
        {
            let service_ep = self.endpoints.get_mut(&service_id_to_endpoint_id(service)).unwrap();
            transition_func(&response, service_ep);
        }
        {
            let method_ep  = self.endpoints.get_mut(&method_id_to_endpoint_id(service, method_id)).unwrap();
            transition_func(&response, method_ep);
        }
    }

    /// updates status field for an endpoint based on the response if the status code was 200
    /// 
    fn handle_response_200_status_transitions(response : &Response, endpoint : &mut Endpoint) {

        assert_eq!(response.status(), StatusCode::OK);

        match &endpoint.status {

            EndpointStatus::Normal => {
                if let Some(cd_state) = endpoint.should_cooldown() {
                    endpoint.status = EndpointStatus::Cooldown(cd_state);
                }
            },

            EndpointStatus::JustOffCooldown(_) | EndpointStatus::Unkown =>
                endpoint.status = EndpointStatus::Normal,

            _ => { panic!("Endpoint was in an invalid state after the query finished!") }
        }
    }

    /// Runs the state transition table for a 429 response on the given endpoint
    /// 
    fn handle_response_429_status_transitions(response : &Response, endpoint : &mut Endpoint) {

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

        match &endpoint.status {

            EndpointStatus::JustOffCooldown(duration) => 
                endpoint.status = EndpointStatus::Cooldown(CooldownState::new(duration.checked_mul(2).unwrap())),

            _ => {}
        }
    }

    /// Uses the response to cache the most-recently seen rate limits from the server
    ///
    /// # Arguments
    /// 
    /// * `response` - a reference to the response given by the lol server (response code must be 200 - ok)
    /// * `region` - the queried region
    /// * `service` - the queried service
    /// * `method_id` - the queried method
    /// 
    /// # Remarks
    /// 
    /// This is used only after receiving a 200 OK and should not be used elsewhere, for it
    /// will panic. This is separately in its own function primarily for convenience/readability.
    fn cache_rate_limits(
        &mut self, response : &Response, region : Region, service : Service, method_id : u32) {

        let date_str = response.headers().get("Date").unwrap().to_str().unwrap();
        let timestamp_ms = DateTime::parse_from_rfc2822(date_str).unwrap().timestamp_millis();


        // cache app limits if more recent
        {
            let region_ep  = self.endpoints.get_mut(&region_id_to_endpoint_id(region)).unwrap();
            if timestamp_ms >= region_ep.last_update_timestamp_ms {

                let rate_limits = response.headers().get("X-App-Rate-Limit").unwrap().to_str().unwrap();
                let rate_limit_counts = response.headers().get("X-App-Rate-Limit-Count").unwrap().to_str().unwrap();

                region_ep.set_buckets_from_headers(rate_limits, rate_limit_counts, timestamp_ms);
                region_ep.last_update_timestamp_ms = timestamp_ms;
            }
        }

        // cache method limits if more recent
        {
            let method_ep  = self.endpoints.get_mut(&method_id_to_endpoint_id(service, method_id)).unwrap();
            if timestamp_ms >= method_ep.last_update_timestamp_ms {

                let rate_limits = response.headers().get("X-Method-Rate-Limit").unwrap().to_str().unwrap();
                let rate_limit_counts = response.headers().get("X-Method-Rate-Limit-Count").unwrap().to_str().unwrap();

                method_ep.set_buckets_from_headers(rate_limits, rate_limit_counts, timestamp_ms);
                method_ep.last_update_timestamp_ms = timestamp_ms;
            }
        }
    }

    /// Updates some internal state prior to making the query to ensure that the endpoint we are about to
    /// query is g2g (e.g. not on cooldown or the lol servers exploded or something)
    /// 
    /// # Arguments
    fn prepare_to_query(
        &mut self, region : Region, service : Service, method_id : u32) -> Result<()>{

        // update + check region
        {
            let region_ep  = self.endpoints.entry(region_id_to_endpoint_id(region))
                .or_insert(Endpoint::new());
            Self::pre_query_update_endpoint(region_ep);
            Self::pre_query_validate_endpoint(region_ep)?;
        }

        // update + check service
        {
            let service_ep  = self.endpoints.entry(service_id_to_endpoint_id(service))
                .or_insert(Endpoint::new());
            Self::pre_query_update_endpoint(service_ep);
            Self::pre_query_validate_endpoint(service_ep)?;
        }

        // update + check method
        {
            let method_ep  = self.endpoints.entry(method_id_to_endpoint_id(service, method_id))
                .or_insert(Endpoint::new());
            Self::pre_query_update_endpoint(method_ep);
            Self::pre_query_validate_endpoint(method_ep)?;
        }

        Ok(())
    }

    fn pre_query_update_endpoint(endpoint : &mut Endpoint) {
        match &endpoint.status {
            EndpointStatus::Cooldown(cd_state) if cd_state.is_expired() => {
                endpoint.status = EndpointStatus::JustOffCooldown(cd_state.duration); //just because we expired, doesn't guarentee normal, the cooldown was a guess
            },
            _ => {}
        }
    }

    fn pre_query_validate_endpoint(endpoint : &mut Endpoint)->Result<()> {
        match &endpoint.status {
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