#[macro_export]
macro_rules! with_trace {
    ($name:expr => $($body:tt)*) => {
        let id = std::thread::current().id();
        let name = $name;
        $crate::log::debug!("{:?}: {}: entering", id, name);
        let res = { $($body)* };
        $crate::log::debug!("{:?}: {}: exiting", id, name);
        res
    }
}

pub use with_trace;
