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
//! 
//! We could have used a tree, but the truthfully this whole thing
//! is statically defined and only changes when the riot api changes
//! so I went with the statically-defined "tree-like" representation
//! to have stronger guarentees of bug-free-"ness" at compile time.

/// used to identify region. Can be readily convered into a u32
/// with the as operator, and is guarenteed to be a safe conversion.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, EnumIter, EnumCount)]
pub enum Region {
    Na1 = 0,
}


/// used to identify the service. Can be readily convered into a u32
/// with the as operator, and is guarenteed to be a safe conversion.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, EnumIter, EnumCount)]
pub enum Service {
    SummonerV4 = 0,
}

const MAX_METHODS_PER_SERVICE : usize = 128; //need this since each service has its own methods enum

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub struct Id(usize);

impl Id {

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
    pub fn from_region(region : Region) -> Self {
        Self(region as usize)
    }

    /// converts a `Service` enum value to its id value in the `endpoints`
    /// HashMap.
    /// 
    /// # Arguments
    /// 
    /// `region` - the region to which the service belongs to
    /// `service` - the `Service` value of the service endpoint
    pub fn from_service(region : Region, service : Service) -> Self {
        let region_idx = region as usize;
        let service_idx = service as usize;
        Self(REGION_COUNT + (region_idx * SERVICE_COUNT) + (service_idx))
    }

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
    /// `service` : the service to which this method belongs
    /// `method` : the u32 representation of the method endpoint 
    ///     (e.g. summoner_v4::Method::ByName as u32)
    pub fn from_method(service : Service, method : u32) -> Self {
        let service_idx = service as usize;
        let method_idx = method as usize;
        Self(REGION_COUNT + (SERVICE_COUNT * REGION_COUNT) + (service_idx * MAX_METHODS_PER_SERVICE) + method_idx)
    }

    /// Given any arbitrary id type, determines if it is a region
    /// id
    /// 
    /// # Arguments
    /// 
    /// `id` - the id to check
    /// 
    /// # Return
    /// 
    /// True if the id belongs to a region endpoint, false otherwise
    pub fn is_region(&self) -> bool {
        self.0 < REGION_COUNT
    }

    /// Given any arbitrary id type, determines if it is a service
    /// id
    /// 
    /// # Arguments
    /// 
    /// `id` - the id to check
    /// 
    /// # Return
    /// 
    /// True if the id belongs to a service endpoint, false otherwise
    #[allow(dead_code)]
    pub fn is_service(&self) -> bool {
        self.0 > REGION_COUNT && self.0 < (REGION_COUNT + (SERVICE_COUNT * REGION_COUNT))
    }

    /// Given any arbitrary id type, determines if it is a method
    /// id
    /// 
    /// # Arguments
    /// 
    /// `id` - the id to check
    /// 
    /// # Return
    /// 
    /// True if the id belongs to a method endpoint, false otherwise
    pub fn is_method(&self) -> bool {
        self.0 > REGION_COUNT + (SERVICE_COUNT * REGION_COUNT)
    }
}