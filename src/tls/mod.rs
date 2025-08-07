#[cfg(feature = "tls-native")]
mod native_tls_impl;

#[cfg(all(any(feature = "__rustls", feature = "__rustls-ring"), not(feature = "tls-native")))]
mod rustls_impl;

#[cfg(all(
    not(feature = "tls-native"),
    not(feature = "__rustls"),
    not(feature = "__rustls-ring")
))]
mod no_tls_impl;

#[cfg(feature = "tls-native")]
pub use native_tls_impl::*;

#[cfg(all(any(feature = "__rustls", feature = "__rustls-ring"), not(feature = "tls-native")))]
pub use rustls_impl::*;

#[cfg(all(
    not(feature = "tls-native"),
    not(feature = "__rustls"),
    not(feature = "__rustls-ring")
))]
pub use no_tls_impl::*;
