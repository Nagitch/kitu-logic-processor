//! Web admin backend skeleton.

use std::collections::HashMap;

use kitu_core::{KituError, Result};

/// Represents a simple HTTP-like request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// Path component of the request (e.g. `/health`).
    pub path: String,
}

/// Represents a simple response structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// Status code returned by the handler.
    pub status: u16,
    /// Response payload body.
    pub body: String,
}

/// Basic backend server with in-memory route handlers.
#[derive(Default)]
pub struct WebAdminServer {
    routes: HashMap<String, Box<dyn Fn(&Request) -> Result<Response> + Send + Sync>>,
    running: bool,
}

impl WebAdminServer {
    /// Registers a route handler.
    pub fn register_route<F>(&mut self, path: impl Into<String>, handler: F)
    where
        F: Fn(&Request) -> Result<Response> + Send + Sync + 'static,
    {
        self.routes.insert(path.into(), Box::new(handler));
    }

    /// Marks the server as running; networking is added later.
    pub fn start(&mut self) {
        self.running = true;
    }

    /// Stops the server.
    pub fn stop(&mut self) {
        self.running = false;
    }

    /// Handles a request using the registered route table.
    pub fn handle(&self, request: Request) -> Result<Response> {
        if !self.running {
            return Err(KituError::NotImplemented("server not started".into()));
        }
        let handler = self
            .routes
            .get(&request.path)
            .ok_or(KituError::InvalidInput("missing route"))?;
        handler(&request)
    }

    /// Whether the server is marked as running.
    pub fn is_running(&self) -> bool {
        self.running
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registering_route_and_handling_request() {
        let mut server = WebAdminServer::default();
        server.register_route("/health", |_req| {
            Ok(Response {
                status: 200,
                body: "ok".into(),
            })
        });
        server.start();
        let response = server
            .handle(Request {
                path: "/health".into(),
            })
            .unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, "ok");
    }

    #[test]
    fn handling_missing_route_errors() {
        let mut server = WebAdminServer::default();
        server.start();
        let result = server.handle(Request {
            path: "/unknown".into(),
        });
        assert!(matches!(
            result,
            Err(KituError::InvalidInput("missing route"))
        ));
    }
}
