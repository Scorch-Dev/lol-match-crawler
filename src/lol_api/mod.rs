
// external uses
use enum_iterator::IntoEnumIterator;
use reqwest::Client;
use std::collections::HashMap;

// my mods
mod endpoint;
use endpoint::{Endpoint, State, SummonerDto};
pub mod errors;
pub use errors::*;

#[derive(Debug)]
pub struct Context {
    endpoints : HashMap<RegionCode, HashMap<EndpointId, Endpoint>>,
    api_key : String,
    client : Client
}

// used to identify endpoints internally
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, IntoEnumIterator)]
enum EndpointId {
    SummonerV4 = 0,
}

// used to identify region
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, IntoEnumIterator)]
pub enum RegionCode {
    Na1 = 0,
}

impl Context {

    pub fn new(api_key : &str) -> Context {
        Context{
            endpoints : RegionCode::into_enum_iter().
                map(|rc|
                    (rc, EndpointId::into_enum_iter().map(|ep| (ep, Endpoint::new())).collect())
                ).collect(),
            api_key : api_key.to_string(),
            client : Client::new(),
        }
    }

    pub fn query_summoner_v4_by_summoner_name(
        &mut self, region_code : RegionCode, summoner_name : &str)->Result<()>{

        // form a valid uri
        let base_uri = Self::base_uri(region_code);
        let query_str = format!("/lol/summoner/v4/summoners/by-name/{summoner_name}", 
            summoner_name=summoner_name);
        let uri = base_uri.to_string() + &query_str;

        // query the endpoint
        let response = self.client.get(&uri);
        println!("{:?}", response);
        Ok(())
    }

    /*
    fn get_endpoint_mut(&mut self, region_code : RegionCode, endpoint_id : EndpointId)->&mut Endpoint {
        self.endpoints.get_mut(&region_code).unwrap().get_mut(&endpoint_id).unwrap()
    }
    */

    fn base_uri(region_code : RegionCode)->&'static str {
        match region_code {
            RegionCode::Na1 => "https://na1.api.riotgames.com"
        }
    }
}