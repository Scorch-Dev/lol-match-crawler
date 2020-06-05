//! A module to enumerate available endpoints via enum
//! and then convert those values to a unique endpoint
//! identifier so that we can store hierarchical endpoints
//! in a single flat data structure like a HashMap or other
//! while maintaining the abstract hierarchy.
//! 
//! The hierarchy is such that each 
//! region has services which have
//! methods. Ids are laid out such that
//! the first Num(regions) IDs for
//! region endpoints, then the next 
//! Num(services) * Num(Regions) IDs for
//! service endpoints (one set per region), 
//! then up to Num(Services) * MAX_METHODS_PER_SERVICE 
//! for each method endpoint after that (one set per service)

pub type IdType = usize;

/// used to identify region. Can be readily convered into a u32
/// with the as operator, and is guarenteed to be a safe conversion.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, EnumIter, EnumCount)]
pub enum Region {
    Na1 = 0,
}

/// converts a `Region` enum value to its id value in the endpoints
/// HashMap.
/// 
/// # Arguments
/// 
/// region : the `Region` value of the region endpoint
/// 
/// # Return
/// 
/// The Id of the endpoint
pub fn region_id(region : Region) -> IdType {
    region as IdType
}


/// used to identify the service. Can be readily convered into a u32
/// with the as operator, and is guarenteed to be a safe conversion.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, EnumIter, EnumCount)]
pub enum Service {
    SummonerV4 = 0,
}

/// converts a `Service` enum value to its id value in the `endpoints`
/// HashMap.
/// 
/// # Arguments
/// 
/// service : the `Service` value of the service endpoint
/// 
/// # Return
/// 
/// The Id of the endpoint
pub fn service_id(region : Region, service : Service) -> IdType {
    let region_idx = region as usize;
    let service_idx = service as usize;
    REGION_COUNT + (region_idx * SERVICE_COUNT) + (service_idx)
}

const MAX_METHODS_PER_SERVICE : usize = 128; //need this since each service has its own methods enum

/// converts a method enum's u32 representation
/// to its id value in the `endpoints` HashMap.
/// 
/// # Remarks
/// 
/// we use the u32 representation of the method
/// since each service has its own methods. E.g.
/// method 0 is different for the service SummonerV4
/// from the method 0 of the League service.
/// 
/// # Arguments
/// 
/// service : the service to which this method belongs
/// method : the u32 representation of the method endpoint 
///     (e.g. summoner_v4::Method::ByName as u32)
/// 
/// # Return
/// 
/// The Id of the endpoint
pub fn method_id(service : Service, method : u32) -> IdType {
    let service_idx = service as usize;
    let method_idx = method as usize;
    REGION_COUNT + (SERVICE_COUNT * REGION_COUNT) + (service_idx * MAX_METHODS_PER_SERVICE) + method_idx
}