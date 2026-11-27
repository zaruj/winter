//! The process-wide TLS crypto provider.
//!
//! kube enables rustls' `ring` provider and the AWS SDK enables `aws-lc-rs`. With both compiled
//! in, rustls refuses to choose and panics the first time a TLS config is built without an
//! explicit provider, which kube's client does. Installing one provider up front avoids that.

use std::sync::Once;

/// Installs `ring` as the default rustls provider. Safe to call any number of times, from any
/// thread; only the first call does anything. Call before creating Kubernetes clients.
pub fn init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // Err only if a provider was already installed, which is just as good.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}
