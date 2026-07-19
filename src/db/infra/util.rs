// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use std::sync::{Mutex, MutexGuard};

pub struct MultiThreadRefCell<T> {
    inner: Mutex<T>,
}

impl<T> MultiThreadRefCell<T> {
    pub fn new(value: T) -> Self {
        Self {
            inner: Mutex::new(value),
        }
    }
    pub fn borrow(&self) -> MutexGuard<'_, T> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn borrow_mut(&self) -> MutexGuard<'_, T> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn try_borrow(&self) -> Option<MutexGuard<'_, T>> {
        self.inner.try_lock().ok()
    }
}
