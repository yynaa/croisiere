use std::{fmt::Debug, hash::Hash};

pub trait ValueObject: Debug + Clone + PartialEq + Eq + Default + Hash {}

impl<V: Debug + Clone + PartialEq + Eq + Default + Hash> ValueObject for V {}