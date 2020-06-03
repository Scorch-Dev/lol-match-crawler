
// external uses
use chrono::DateTime;
use reqwest::blocking::{Client, Response};
use reqwest::StatusCode;
use std::collections::HashMap;

// my mods
mod services;
mod endpoint;
mod errors;

pub use errors::*;
use services::summoner_v4;
use endpoint::Endpoint;


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
        self.handle_response(response, region, service, method_id)
    }

    /// Call this after the query is sent to handle any internal state
    /// updates using the response.
    /// 
    /// > **NOTE**: this will consume the response proivded so call it last
    /// 
    fn handle_response(
        &mut self, response : Response, region : Region, service : Service, method_id : u32) -> Result<Response> {
        
        // do any extra work or update internal state first
        match response.status() {
            StatusCode::OK => self.cache_rate_limits(&response, region, service, method_id)?,
            _ => { }
        }

        //now that internal state is updated, make a state transition for endpoints
        self.handle_status_transitions(response.status(), region, service, method_id);

        // convert to error if required
        match response.error_for_status() {
            Err(e) => { println!("{:?}", self.endpoints.get(&region_id_to_endpoint_id(region))); Err(Error::from(e)) },
            Ok(r) => Ok(r),
        }
    }

    /// Helper method to avoid retyping the same thing over and over. Takes a state transition function
    /// and applies it to each of the endpoitns specified by region, service, and method. The
    /// transition function uses the result and the current status of any given endpoint to alter the endpoints
    /// current status.
    /// 
    fn handle_status_transitions(&mut self, 
        status_code : StatusCode, region : Region, service : Service, method_id : u32){

        {
            let region_ep  = self.endpoints.get_mut(&region_id_to_endpoint_id(region)).unwrap();
            region_ep.update_status_from_response_code(status_code);
        }
        {
            let service_ep = self.endpoints.get_mut(&service_id_to_endpoint_id(service)).unwrap();
            service_ep.update_status_from_response_code(status_code);
        }
        {
            let method_ep  = self.endpoints.get_mut(&method_id_to_endpoint_id(service, method_id)).unwrap();
            method_ep.update_status_from_response_code(status_code);
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
        &mut self, response : &Response, region : Region, service : Service, method_id : u32) -> Result<()> {

        let date_str = response.headers().get("Date").unwrap().to_str().unwrap();
        let timestamp_ms = DateTime::parse_from_rfc2822(date_str).unwrap().timestamp_millis();


        // cache app limits if more recent
        {
            let region_ep  = self.endpoints.get_mut(&region_id_to_endpoint_id(region)).unwrap();
            if timestamp_ms >= region_ep.last_update_timestamp_ms() {

                let limits = Self::get_header_as_rate_limit(&response, "X-App-Rate-Limit")?;
                let counts = Self::get_header_as_rate_limit(&response, "X-App-Rate-Limit-Count")?;

                region_ep.update_buckets(&limits, &counts, timestamp_ms);
            }
        }

        // cache method limits if more recent
        {
            let method_ep  = self.endpoints.get_mut(&method_id_to_endpoint_id(service, method_id)).unwrap();
            if timestamp_ms >= method_ep.last_update_timestamp_ms() {

                let limits = Self::get_header_as_rate_limit(&response, "X-Method-Rate-Limit")?;
                let counts = Self::get_header_as_rate_limit(&response, "X-Method-Rate-Limit-Count")?;

                method_ep.update_buckets(&limits, &counts, timestamp_ms);
            }
        }

        Ok(())
    }

    /// A little helper to do some error checking while we get the header
    /// and reduce verbosity/typing in other functions
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
    fn prepare_to_query(
        &mut self, region : Region, service : Service, method_id : u32) -> Result<()>{

        // update + check region
        {
            let region_ep  = self.endpoints.entry(region_id_to_endpoint_id(region))
                .or_insert(Endpoint::new());
            region_ep.update_status_pre_query();
            if region_ep.can_query() == false { return Err(Error::from(ErrorKind::from(region_ep.status()))); }
        }

        // update + check service
        {
            let service_ep  = self.endpoints.entry(service_id_to_endpoint_id(service))
                .or_insert(Endpoint::new());
            service_ep.update_status_pre_query();
            if service_ep.can_query() == false { return Err(Error::from(ErrorKind::from(service_ep.status()))); }
        }

        // update + check method
        {
            let method_ep  = self.endpoints.entry(method_id_to_endpoint_id(service, method_id))
                .or_insert(Endpoint::new());
            method_ep.update_status_pre_query();
            if method_ep.can_query() == false { return Err(Error::from(ErrorKind::from(method_ep.status()))); }
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