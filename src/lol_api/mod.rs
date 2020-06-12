//! The lol_api module is a way to interface with the
//! league of legends api in a friendly way that takes into
//! account rate limits and bad responses. Currently,
//! none of these methods are thread-safe. 
//! 
//! Most of the methods are for querying the api without
//! having to deal with things like Http or networking
//! code or excessive error checking and rate-limit
//! checking.

// external uses
use chrono::DateTime;
use reqwest::{Client, Response};
use reqwest::StatusCode;
use std::collections::HashMap;
use tokio::sync::Mutex;

// my mods/uses
mod services;
mod endpoint;
mod errors;

pub use errors::*;
pub use endpoint::{Region, Service};

use services::summoner_v4;
use endpoint::{Endpoint, Id};

/// The context we construct to guess the state
/// of the various endpoints within the league of legends
/// api. We can use the context to make queries to the
/// api in a safer, easier manner while keeping track
/// of rate limits and such.
#[derive(Debug)]
pub struct Context {
    endpoints : Mutex<HashMap<Id, Endpoint>>,  // now the whole struct is sync, hurray!
    api_key : String,
    client : Client
}

impl Context {

    /// Constructs a new lol api
    /// 
    pub fn new(api_key : &str) -> Context {
        Context{
            endpoints : Mutex::new(HashMap::new()),
            api_key : api_key.to_string(),
            client : Client::new(),
        }
    }

    /** SUMMONER V4 METHODS */
    #[allow(dead_code)]
    pub async fn query_summoner_v4_by_summoner_name(
        &self, region : Region, summoner_name : &str, retry_count : usize)->Result<summoner_v4::SummonerDto> {

        Self::query_with_retry(retry_count, 
            &||{ self.try_query_summoner_v4_by_summoner_name(region, summoner_name) } ).await
    }

    pub async fn try_query_summoner_v4_by_summoner_name(
        &self, region : Region, summoner_name : &str)->Result<summoner_v4::SummonerDto>{

        let uri = Self::region_uri(region) + &summoner_v4::by_name_uri(summoner_name);
        let endpoint_ids = [Id::from_region(region), 
                            Id::from_service(region, Service::SummonerV4), 
                            Id::from_method(Service::SummonerV4, summoner_v4::Method::ByName as u32)];
        let response = self.send_query(&uri, &endpoint_ids).await?;
        let data = response.json::<summoner_v4::SummonerDto>().await?;
        Ok(data)
    }

    #[allow(dead_code)]
    pub async fn query_summoner_v4_by_account(
        &self, region : Region, encrypted_account_id : &str, retry_count : usize)->Result<summoner_v4::SummonerDto> {

        Self::query_with_retry(retry_count, 
            &|| { self.try_query_summoner_v4_by_account(region, encrypted_account_id) }).await
    }

    pub async fn try_query_summoner_v4_by_account(
        &self, region : Region, encrypted_account_id : &str)->Result<summoner_v4::SummonerDto> {

        let uri = Self::region_uri(region) + &summoner_v4::by_account_uri(encrypted_account_id);
        let endpoint_ids = [Id::from_region(region), 
                            Id::from_service(region, Service::SummonerV4), 
                            Id::from_method(Service::SummonerV4, summoner_v4::Method::ByAccount as u32)];
        let response = self.send_query(&uri, &endpoint_ids).await?;
        let data = response.json::<summoner_v4::SummonerDto>().await?;
        Ok(data)
    }

    /// A helper which takes an async closure to save on typing for the
    async fn query_with_retry<T, F>(retry_count : usize, query_func : &dyn Fn() -> F ) -> Result<T> 
    where F : std::future::Future<Output=Result<T>>{

        let mut res = Err(Error::from(""));

        for i in 0..(retry_count+1) {
            res = query_func().await;
            match &res {
                Ok(_) => break,
                Err(e) if e.can_retry() && i < retry_count => { 
                    tokio::time::delay_for(e.retry_time().unwrap()).await;
                },
                _ => {},
            }
        }

        res.chain_err(|| "Retry count exceeded")
    }

    /// The workhorse method for synhrnous querying. We check internal state
    /// make sure the query is safe to execute (e.g. the endpoint isn't on cooldown and we can send),
    /// sends the request, blocks, caches rate-limiting related information,
    /// then returns the server response. If anything happens along the way or the server responds with
    /// anything but 200 - OK we return the error.
    /// 
    /// # Arguments
    /// 
    /// `uri` - the uri to execute the GET request against
    /// `endpoint_ids` - identifiers of affected endpoints
    /// 
    /// # Remarks
    /// 
    /// This is the primary yeild point that could lead to panics in our `endpoints`
    /// member, which is a `RefCell`. The only thing to watch out for is to make
    /// sure that we are not holding the lock when we yield out or risk poisoning the mutex on the endpoints.
    /// 
    /// # Return
    /// 
    /// A result indicating the reqwest::Response 
    /// if one was received from the server (otherwise an error)
    async fn send_query(&self, uri : &str, endpoint_ids : &[Id])->Result<Response> {

        self.prepare_to_query(&endpoint_ids).await?;
        let response = self.client.get(uri)
            .header("X-Riot-Token", &self.api_key)
            .send().await?;
        self.handle_response(response, endpoint_ids).await
    }

    /// Call this after the query is sent to handle any internal state
    /// updates using the response.
    /// 
    /// > **NOTE**: this will consume the response proivded so call it last
    /// 
    /// # Arguments
    /// 
    /// `response` : the server response
    /// `endpoint_ids` : the identifiers for the affected endpoints
    /// 
    /// # Return
    /// 
    /// A `Result`, which is the `Response` provided as an argument 
    /// if there was no error, otherwise returns the error.
    async fn handle_response(
        &self, response : Response, endpoint_ids : &[Id]) -> Result<Response> {
        
        // do any extra work or update internal state first
        match response.status() {
            StatusCode::OK => self.cache_rate_limits(&response, endpoint_ids).await?,
            _ => { }
        }

        //now that internal state is updated, make a state transition for endpoints
        self.handle_status_transitions(response.status(), endpoint_ids).await;

        // convert to error if required
        // TODO: find the offending endpoint and get the likely cooldown
        match response.error_for_status() {
            Ok(r) => Ok(r),
            Err(e) => Err(Error::from(e)),
        }
    }

    /// Helper method to avoid retyping the same thing over and over. Takes a state transition function
    /// and applies it to each of the endpoitns specified by region, service, and method. The
    /// transition function uses the result and the current status of any given endpoint to alter the endpoints
    /// current status.
    /// 
    /// # Arguments
    /// 
    /// `status_code` : the status code the server responded with
    /// `endpoint_ids` : the identifiers for the affected endpoints
    async fn handle_status_transitions(&self, 
        status_code : StatusCode, endpoint_ids : &[Id]){

        let endpoints_ref = &mut self.endpoints.lock().await;

        for id in endpoint_ids {
            let ep  = endpoints_ref.get_mut(id).unwrap();
            ep.update_status_from_response_code(status_code);
        }
    }

    /// Uses the response to cache the 
    /// most-recently seen rate limits from the server
    /// This method mainly massages the 
    /// inputs and does parsing so the
    /// buckets can be updated properly
    ///
    /// # Arguments
    /// 
    /// * `response` - a reference to the response 
    ///     given by the lol server (response code must be 200 - ok)
    /// `endpoint_ids` : the identifiers for the affected endpoints
    /// 
    /// # Remarks
    /// 
    /// This is used only after receiving a 200 OK and should not be used elsewhere, for it
    /// will panic. This is separately in its own function primarily for convenience/readability.
    async fn cache_rate_limits(
        &self, response : &Response, endpoint_ids : &[Id]) -> Result<()> {

        let endpoints_ref = &mut self.endpoints.lock().await;

        let date_str = response.headers().get("Date").unwrap().to_str().unwrap();
        let timestamp_ms = DateTime::parse_from_rfc2822(date_str).unwrap().timestamp_millis();

        // cache app limits if more recent
        for id in endpoint_ids {

            // use the appropriate header for region endpoint rate limiting
            if id.is_region() {
                let region_ep  = endpoints_ref.get_mut(id).unwrap();
                if timestamp_ms >= region_ep.last_update_timestamp_ms() {

                    let limits = Self::get_header_as_rate_limit(&response, "X-App-Rate-Limit")?;
                    let counts = Self::get_header_as_rate_limit(&response, "X-App-Rate-Limit-Count")?;

                    region_ep.update_buckets(&limits, &counts, timestamp_ms);
                }
            }
            // use the appropriate header for method endpoint rate limiting
            else if id.is_method() {
                let method_ep  = endpoints_ref.get_mut(id).unwrap();
                if timestamp_ms >= method_ep.last_update_timestamp_ms() {

                    let limits = Self::get_header_as_rate_limit(&response, "X-Method-Rate-Limit")?;
                    let counts = Self::get_header_as_rate_limit(&response, "X-Method-Rate-Limit-Count")?;

                    method_ep.update_buckets(&limits, &counts, timestamp_ms);
                }
            }
        }

        Ok(())
    }

    /// A little helper to do some error checking while we get the header
    /// and reduce verbosity/typing in other functions
    /// 
    /// # Arguments
    /// 
    /// `response` : the `Response` object we received
    /// `header_name` : the name of the header to pull
    /// 
    /// # Return
    /// 
    /// The header value as a new String object or an error
    /// if the conversion failed.
    fn get_header_as_str(response : &Response, header_name : &str) -> Result<String> {

        let header_val = response.headers().get(header_name)
                         .chain_err(|| format!("Header {} not found.", header_name))?;
        Ok(header_val.to_str()?.to_string())
    }
    
    /// Takes a formatted rate limit string from the response header
    /// and parses it to u64 pair. format is
    /// `<item1>:<item2>,<item3>:<item4>` in general. That is items
    /// are separated by `:` and pairs are separated by `,`
    /// 
    /// # Arguments
    /// 
    /// `response` : the `Response` object we received
    /// `header_name` : the name of the header to pull
    /// 
    /// # Return
    /// 
    /// The header value as a Vec(limit,bucket_size) on success
    /// or an error if the parse failed.
    fn get_header_as_rate_limit(response : &Response, header_name : &str) -> Result<Vec<(u64,u64)>> {
        
        let limit_str = Self::get_header_as_str(&response, header_name)?;

        limit_str.split(",")
            .map(|item| {
                let mut split = item.split(":");
                
                if let (Some(first), Some(second)) = (split.next(), split.next()) {
                    let n1 = first.parse::<u64>().chain_err(|| "Could not parse rate limit string!")?;
                    let n2 = second.parse::<u64>().chain_err(|| "Could not parse rate limit string!")?;
                    Ok((n1,n2))
                }
                else {
                    Err(Error::from("Could not parse rate limit string!"))
                }
            }).collect()
    }

    /// Updates some internal state prior to making the query to ensure that the endpoint we are about to
    /// query is g2g (e.g. not on cooldown or the lol servers exploded or something)
    /// 
    /// # Arguments
    /// 
    /// `endpoint_ids` : the identifiers for the affected endpoints
    /// 
    /// # Return
    /// 
    /// Gives a `Result` containin `()` on success, and
    /// an error on failure.
    async fn prepare_to_query(
        &self, endpoint_ids : &[Id]) -> Result<()>{

        // update + check region
        for id in endpoint_ids {
            let endpoints_ref = &mut self.endpoints.lock().await;
            let ep  = endpoints_ref.entry(*id).or_insert(Endpoint::new());
            ep.update_status_pre_query();
            if ep.can_query() == false { 
                return Err(Error::from(ErrorKind::from(ep.status()))); 
            }
        }

        Ok(())
    }

    /// Takes the region enum and provides the formatted uri
    /// that prefixes calls to services in this region
    /// 
    /// #Arguments
    /// 
    /// `region` - the region to construct a query prefix string for
    /// 
    /// #Return
    /// 
    /// The formatted uri for the api 
    /// (e.g. https://na1.api.riotgames.com)
    fn region_uri(region : Region)->String {
        format!("https://{:?}.api.riotgames.com", region)
    }
}