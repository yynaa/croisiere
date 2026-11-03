use std::fmt::Debug;

pub trait Entity: Debug + Default {}

impl<E: Debug + Default> Entity for E {}