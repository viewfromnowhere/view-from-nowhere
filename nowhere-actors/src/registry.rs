use crate::actor::{Actor, Addr};
use dashmap::DashMap;
use std::{
    any::{Any, TypeId},
    sync::Arc,
};

/// Thread-safe registry for sharing typed values (usually `Addr<T>`).
///
/// Necessity:
/// - Late binding: components can be wired after spawn.
/// - Avoids monolithic constructors & brittle global singletons.
#[derive(Default, Clone)]
pub struct Registry {
    by_name: Arc<DashMap<String, Box<dyn Any + Send + Sync>>>,
    by_type: Arc<DashMap<TypeId, Box<dyn Any + Send + Sync>>>,
}

impl Registry {
    pub fn insert_named<T: Send + Sync + 'static>(&self, name: impl Into<String>, value: T) {
        self.by_name.insert(name.into(), Box::new(value));
    }

    pub fn insert<T: Send + Sync + 'static>(&self, value: T) {
        self.by_type.insert(TypeId::of::<T>(), Box::new(value));
    }

    pub fn get_named<T: Send + Sync + 'static + Clone>(&self, name: &str) -> Option<T> {
        self.by_name.get(name)?.downcast_ref::<T>().cloned()
    }

    pub fn get<T: Send + Sync + 'static + Clone>(&self) -> Option<T> {
        self.by_type
            .get(&TypeId::of::<T>())?
            .downcast_ref::<T>()
            .cloned()
    }

    pub fn insert_addr<A: Actor>(&self, name: &str, addr: Addr<A>)
    where
        Addr<A>: Clone + Send + Sync + 'static,
    {
        let key = format!("{}::{}", std::any::type_name::<Addr<A>>(), name);
        self.insert_named(key, addr);
    }
    pub fn get_addr<A: Actor>(&self, name: &str) -> Option<Addr<A>>
    where
        Addr<A>: Clone + Send + Sync + 'static,
    {
        let key = format!("{}::{}", std::any::type_name::<Addr<A>>(), name);
        self.get_named(&key)
    }
}
