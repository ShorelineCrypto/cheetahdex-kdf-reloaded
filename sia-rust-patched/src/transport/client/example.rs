// THIS FILE MUST NOT INCLUDED IN WORKSPACE
**Some Broken Code to Ensure the Build Fails if this File is Included in the Workspace**

use crate::transport::endpoints::{SiaApiRequest};
use async_trait::async_trait;
use serde::Deserialize;
use crate::transport::client::{ApiClient, ApiClientError, ApiClientHelpers, EndpointSchema};


#[derive(Clone)]
pub struct ExampleClient {
    pub client: String,  // Placeholder for a client type, generally something allowing data requests
}

// Placeholder configuration struct for ExampleClient
// Generally this will hold everything neccesary to init a new instance of ExampleClient
// It can include things like authentication, timeouts, etc.
#[derive(Clone, Debug, Deserialize)]
pub struct ExampleClientConf {
    pub foo: String,                       // Placeholders for some client specific configurations
    pub bar: Option<String>,               
}

#[async_trait]
impl ApiClient for ExampleClient {
    type Request = PlaceholderRequest;   // Placeholder for the request type, like `reqwest::Request`
    type Response = PlaceholderResponse; // Placeholder for the response type, like `reqwest::Response`
    type Conf = ExampleClientConf;       // Configuration type for ExampleClient

    // Instantiate a new ExampleClient
    async fn new(conf: Self::Conf) -> Result<Self, ApiClientError> {
        // Example of how you might process authentication, timeouts, etc.
        todo!(); // Replace with the logic needed to build ExampleClient
    }

    // Process an `EndpointSchema` and convert it into a request
    fn process_schema(&self, schema: EndpointSchema) -> Result<Self::Request, ApiClientError> {
        /// Add logic for converting the schema into a request
        /// Schema is a standard format for providing a client with the information needed
        /// to create their request type. 
        todo!(); 
    }

    // Convert an `SiaApiRequest` into a `Request`
    fn to_data_request<R: SiaApiRequest>(&self, request: R) -> Result<Self::Request, ApiClientError> {
        /// Convert an `SiaApiRequest` into a `Request` that can be executed
        /// SiaApiRequest represents a request-response pair defining data types for input and output
        /// This function converts the request into a format that ExampleClient can execute
        let schema = request.to_endpoint_schema()?;  // Convert request to schema
        self.process_schema(schema)                  // Process schema into a request
    }

    // Execute the request and return a response
    async fn execute_request(&self, request: Self::Request) -> Result<Self::Response, ApiClientError> {
        // eg, self.client().execute(request).await
        todo!();
    }

    // Dispatcher function that converts the request and handles execution
    async fn dispatcher<R: SiaApiRequest>(&self, request: R) -> Result<R::Response, ApiClientError> {
        let request = self.to_data_request(request)?;  // Convert request to data request

        // Execute the request
        let response = self.execute_request(request).await?;
        
        // Check the response status and return the appropriate result
        todo!(); // Handle status and response, similar to NativeClient's dispatcher
    }
}

// Implement the optional helper methods for ExampleClient
// Just this is needed to implement the `ApiClientHelpers` trait
// unless custom implementations for the traits methods are needed
#[async_trait]
impl ApiClientHelpers for ExampleClient {}