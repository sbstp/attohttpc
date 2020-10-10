#[cfg(feature = "tls")]
mod native_tls_impl;

#[cfg(all(feature = "tls-rustls", not(feature = "tls")))]
mod rustls_impl;

#[cfg(all(not(feature = "tls"), not(feature = "tls-rustls")))]
mod no_tls_impl;

#[cfg(feature = "tls")]
pub use native_tls_impl::*;

#[cfg(all(feature = "tls-rustls", not(feature = "tls")))]
pub use rustls_impl::*;

#[cfg(all(not(feature = "tls"), not(feature = "tls-rustls")))]
pub use no_tls_impl::*;
