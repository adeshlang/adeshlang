use super::super::errors::HttpError;
use super::super::response::Response;
use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};

#[derive(Clone)]
struct FlightCall {
    done: bool,
    res: Option<Result<Response, HttpError>>,
}

#[derive(Clone, Default)]
pub struct SingleFlight {
    calls: Arc<Mutex<HashMap<String, (Arc<Mutex<FlightCall>>, Arc<Condvar>)>>>,
}

impl SingleFlight {
    pub fn new() -> Self {
        Self {
            calls: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn execute<F>(&self, key: &str, f: F) -> Result<Response, HttpError>
    where
        F: FnOnce() -> Result<Response, HttpError>,
    {
        let mut map = self.calls.lock().unwrap();
        if let Some((call_lock, cvar)) = map.get(key).cloned() {
            drop(map);
            let mut guard = call_lock.lock().unwrap();
            while !guard.done {
                guard = cvar.wait(guard).unwrap();
            }
            return guard.res.as_ref().unwrap().clone();
        }

        let call_lock = Arc::new(Mutex::new(FlightCall {
            done: false,
            res: None,
        }));
        let cvar = Arc::new(Condvar::new());
        map.insert(key.to_string(), (call_lock.clone(), cvar.clone()));
        drop(map);

        let res = f();

        let mut guard = call_lock.lock().unwrap();
        guard.res = Some(res.clone());
        guard.done = true;
        cvar.notify_all();
        drop(guard);

        let mut map = self.calls.lock().unwrap();
        map.remove(key);

        res
    }
}
