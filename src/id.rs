use std::fmt;
use std::marker::PhantomData;

pub trait IdGenerator {
  fn next_id() -> u32;
}

pub struct ID<T: IdGenerator> {
  pub inner: u32,
  _phantom: PhantomData<T>,
}

impl<T: IdGenerator> ID<T> {
  pub fn generate() -> Self {
    Self {
      inner: T::next_id(),
      _phantom: PhantomData,
    }
  }
}

impl<T: IdGenerator> Clone for ID<T> {
  fn clone(&self) -> Self {
    *self
  }
}

impl<T: IdGenerator> Copy for ID<T> {}

impl<T: IdGenerator> PartialEq for ID<T> {
  fn eq(&self, other: &Self) -> bool {
    self.inner == other.inner
  }
}

impl<T: IdGenerator> Eq for ID<T> {}

impl<T: IdGenerator> std::hash::Hash for ID<T> {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.inner.hash(state);
  }
}

impl<T: IdGenerator> fmt::Debug for ID<T> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let type_name = std::any::type_name::<T>();
    let short_name = type_name.rsplit("::").next().unwrap_or(type_name);
    f.debug_struct(&format!("ID<{}>", short_name))
      .field("inner", &self.inner)
      .finish()
  }
}

#[macro_export]
macro_rules! impl_id {
  ($type:ty) => {
    impl $crate::id::IdGenerator for $type {
      fn next_id() -> u32 {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT_ID: AtomicU32 = AtomicU32::new(1);
        NEXT_ID.fetch_add(1, Ordering::SeqCst)
      }
    }
  };
}
