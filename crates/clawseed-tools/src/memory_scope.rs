use clawseed_api::memory_traits::Memory;

use clawseed_memory::namespaced::PUBLIC_NAMESPACE;

pub(crate) fn private_namespace(memory: &dyn Memory) -> String {
    memory
        .accessible_namespaces()
        .into_iter()
        .find(|namespace| namespace != PUBLIC_NAMESPACE)
        .unwrap_or_else(|| "default".into())
}
