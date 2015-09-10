// The MIT License (MIT)
//
// Copyright (c) 2015  Nick Stevens <nick@bitcurry.com>
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

#![crate_type = "lib"]
#![crate_name = "fixedvec"]

#![feature(core)]

//! Heapless Vec implementation using only libcore
//!
//! When developing for certain types of systems, especially embedded systems,
//! it is desirable to avoid the non-determinism that can be introduced by
//! using a heap. A commonly used data structure is a "buffer" - a
//! preallocated chunk of memory, either in static memory or on the stack.
//!
//! Thanks to the extensibility of Rust, it is possible to have a datatype
//! that performs _almost_ like the libstd `Vec` type, without requiring a
//! heap and while only using libcore.
//!
//! # Differences from `std::vec::Vec`
//!
//! For now, `FixedVec` only works for types that implement `Copy`. This
//! requirement will be lifted in the future, but for now it is the most
//! straightforward way to get to a minimum viable product.
//!
//! Although every effort has been made to mimic the functionality of `Vec`,
//! this is not a perfect clone. Specifically, functions that require memory
//! allocation are not included. There are also a few functions where the type
//! signatures are different - usually to add a `Result` that indicates whether
//! or not adding an element was successful.
//!
//! Note that the `Vec` functionality of panicking when an invalid index is
//! accessed has been preserved. Note that this is true _even if the index is
//! valid for the underlying memory_. So, for example, if a `FixedVec` were
//! allocated with 10 elements, and 3 new elements were pushed to it, accessing
//! index 5 would panic, even though accessing that memory would be safe.
//!
//! ## Functions with different signatures
//!
//! The following functions have different signatures than their equivalents in
//! `Vec`.
//!
//! * `new`: Self-explanatory - instantiating a different object
//! * `push`, `push_all`, `insert`: Functions that add elements return a Result
//!    indicating if the result was successful.
//! * `map_in_place`: Similar to `Vec` `map_in_place`, except there is no
//!    coercion of the types.
//!
//! ## Functions in `FixedVec` not in `Vec`
//!
//! * `available`: Convenience function for checking remaining space.
//! * `iter`: `FixedVec` cannot implement `IntoIterator` because the type
//!   signature of that trait requires taking ownership of the underlying
//!   struct. Since `FixedVec` keeps a reference to its backing store,
//!   ownership is not its to give. It's possible I'm just being dense and this
//!   is possible - I'd love to be proven wrong.
//!
//! ## Functions in `Vec` excluded from `FixedVec`
//!
//! The following `Vec` functions do not exist in `FixedVec` because they deal
//! with allocating or reserving memory - a step that is done up-front in
//! `FixedVec`.
//!
//! * `with_capacity`
//! * `from_raw_parts`
//! * `from_raw_buffer`
//! * `reserve`
//! * `reserve_exact`
//! * `shrink_to_fit`
//! * `into_boxed_slice`
//! * `truncate`
//! * `set_len`
//! * `append`
//! * `drain`
//! * `split_off`
//!
//! # Examples
//!
//! Typical usage looks like the following:
//!
//! ```rust
//! #![feature(core)]
//! extern crate core;
//!
//! #[macro_use] extern crate fixedvec;
//!
//! use fixedvec::FixedVec;
//!
//! fn main() {
//!     let mut preallocated_space = alloc_stack!([u8; 10]);
//!     let mut vec = FixedVec::new(&mut preallocated_space);
//!     assert_eq!(vec.len(), 0);
//!
//!     vec.push_all(&[1, 2, 3]).unwrap();
//!     assert_eq!(vec.len(), 3);
//!     assert_eq!(vec[1], 2);
//!
//!     vec[1] = 5;
//!     assert_eq!(vec[1], 5);
//! }
//! ```

extern crate core;

use core::hash::{Hash, Hasher};
use core::ops;

/// Convenience macro for use with `FixedVec`. Allocates the specified number
/// of elements of specified type on the stack.
///
/// # Example
///
/// ```
/// # #[macro_use] extern crate fixedvec;
/// # use fixedvec::FixedVec;
/// # fn main() {
/// // Allocate space for 16 u8's
/// let mut space = alloc_stack!([u8; 16]);
///
/// // Give the space to a `FixedVec`, which manages it from here on out
/// let vec = FixedVec::new(&mut space);
/// # }
/// ```
#[macro_export]
macro_rules! alloc_stack {
    ([$item_type:ty; $len:expr]) => ({
        let space: [$item_type; $len] = [ Default::default() ; $len ];
        space
    })
}

pub type Result<T> = core::result::Result<T, ErrorKind>;

#[derive(Debug)]
pub enum ErrorKind {
    NoSpace,
}

#[derive(Debug)]
pub struct FixedVec<'a, T: 'a + Copy> {
    memory: &'a mut [T],
    len: usize,
}

pub struct Iter<'a, T: 'a + Copy> {
    list: &'a FixedVec<'a, T>,
    idx: usize,
}

impl <'a, T: 'a + Copy> FixedVec<'a, T> {
    /// Create a new `FixedVec` from the provided slice, in the process taking
    /// ownership of the slice.
    ///
    /// # Example
    ///
    /// ```
    /// # #[macro_use] extern crate fixedvec;
    /// # use fixedvec::FixedVec;
    /// # fn main() {
    /// let mut space = alloc_stack!([u8; 16]);
    /// let vec = FixedVec::new(&mut space);
    /// assert_eq!(vec.capacity(), 16);
    /// assert_eq!(vec.len(), 0);
    /// assert_eq!(&[] as &[u8], vec.as_slice());
    /// # }
    /// ```
    ///
    pub fn new(memory: &'a mut [T]) -> Self {
        FixedVec {
            memory: memory,
            len: 0,
        }
    }

    /// Returns the capacity of the vector.
    ///
    /// # Example
    ///
    /// ```
    /// # #[macro_use] extern crate fixedvec;
    /// # use fixedvec::FixedVec;
    /// # fn main() {
    /// let mut space = alloc_stack!([u8; 16]);
    /// let mut vec = FixedVec::new(&mut space);
    /// assert_eq!(vec.capacity(), 16);
    /// vec.push(1).unwrap();
    /// assert_eq!(vec.capacity(), 16);
    /// # }
    /// ```
    pub fn capacity(&self) -> usize {
        self.memory.len()
    }

    /// Returns the number of elements in the vector. This will always be
    /// less than or equal to the `capacity()`.
    ///
    /// # Example
    ///
    /// ```
    /// # #[macro_use] extern crate fixedvec;
    /// # use fixedvec::FixedVec;
    /// # fn main() {
    /// let mut space = alloc_stack!([u8; 16]);
    /// let mut vec = FixedVec::new(&mut space);
    /// vec.push(1).unwrap();
    /// vec.push(2).unwrap();
    /// assert_eq!(vec.len(), 2);
    /// # }
    /// ```
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns the number of available elements in the vector. Adding more
    /// than this number of elements (without removing some elements) will
    /// cause further calls to element-adding functions to fail.
    ///
    /// # Example
    ///
    /// ```
    /// # #[macro_use] extern crate fixedvec;
    /// # use fixedvec::FixedVec;
    /// # fn main() {
    /// let mut space = alloc_stack!([u8; 16]);
    /// let mut vec = FixedVec::new(&mut space);
    /// assert_eq!(vec.available(), 16);
    /// vec.push(1).unwrap();
    /// assert_eq!(vec.available(), 15);
    /// assert_eq!(vec.available(), vec.capacity() - vec.len());
    /// # }
    /// ```
    #[inline]
    pub fn available(&self) -> usize {
        self.capacity() - self.len()
    }

    /// Returns `true` if the vector contains no elements.
    ///
    /// # Example
    ///
    /// ```
    /// # #[macro_use] extern crate fixedvec;
    /// # use fixedvec::FixedVec;
    /// # fn main() {
    /// let mut space = alloc_stack!([u8; 16]);
    /// let mut vec = FixedVec::new(&mut space);
    /// assert!(vec.is_empty());
    /// vec.push(1);
    /// assert!(!vec.is_empty());
    /// # }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Extracts a slice containing the entire vector.
    ///
    /// Equivalent to `&s[..]`.
    ///
    /// # Example
    ///
    /// ```
    /// # #[macro_use] extern crate fixedvec;
    /// # use fixedvec::FixedVec;
    /// # fn main() {
    /// let mut space = alloc_stack!([u8; 16]);
    /// let mut vec = FixedVec::new(&mut space);
    ///
    /// vec.push_all(&[1, 2, 3, 4]).unwrap();
    /// assert_eq!(vec.as_slice(), &[1, 2, 3, 4]);
    /// # }
    #[inline]
    pub fn as_slice(&self) -> &[T] {
        &self.memory[..self.len]
    }

    /// Extracts a mutable slice of the entire vector.
    ///
    /// Equivalent to `&mut s[..]`.
    ///
    /// # Example
    ///
    /// ```
    /// # #[macro_use] extern crate fixedvec;
    /// # use fixedvec::FixedVec;
    /// # fn main() {
    /// let mut space = alloc_stack!([u8; 16]);
    /// let mut vec = FixedVec::new(&mut space);
    ///
    /// vec.push(1).unwrap();
    /// let mut slice = vec.as_mut_slice();
    /// slice[0] = 2;
    /// assert_eq!(slice[0], 2);
    /// # }
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.memory[..self.len]
    }

    /// Inserts an element at position `index` within the vector, shifting all
    /// elements after position `i` one position to the right.
    ///
    /// # Panics
    ///
    /// Panics if `index` is greater than the vector's length.
    ///
    /// # Example
    ///
    /// ```
    /// # #[macro_use] extern crate fixedvec;
    /// # use fixedvec::FixedVec;
    /// # fn main() {
    /// let mut space = alloc_stack!([u8; 5]);
    /// let mut vec = FixedVec::new(&mut space);
    ///
    /// // Inserting in the middle moves elements to the right
    /// vec.push_all(&[1, 2, 3]).unwrap();
    /// vec.insert(1, 15).unwrap();
    /// assert_eq!(vec.as_slice(), &[1, 15, 2, 3]);
    ///
    /// // Can also insert at the end of the vector
    /// vec.insert(4, 16).unwrap();
    /// assert_eq!(vec.as_slice(), &[1, 15, 2, 3, 16]);
    ///
    /// // Cannot insert if there is not enough capacity
    /// assert!(vec.insert(2, 17).is_err());
    /// # }
    pub fn insert(&mut self, index: usize, element: T) -> Result<()> {
        if index > self.capacity() {
            panic!("Index out of bounds");
        } else if self.len() == 0 {
            self.push(element)
        } else if self.available() >= 1 {
            self.len += 1;
            let mut i = self.len;
            loop {
                if i == index { break; }
                self.memory[i] = self.memory[i - 1];
                i -= 1;
            }
            self.memory[index] = element;
            Ok(())
        } else {
            Err(ErrorKind::NoSpace)
        }
    }

    /// Removes and returns the element at position `index` within the vector,
    /// shifting all elements after position `index` one position to the left.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of bounds.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate fixedvec;
    /// # use fixedvec::FixedVec;
    /// # fn main() {
    /// let mut space = alloc_stack!([u8; 16]);
    /// let mut vec = FixedVec::new(&mut space);
    ///
    /// // Remove element from the middle
    /// vec.push_all(&[1, 2, 3]).unwrap();
    /// assert_eq!(vec.remove(1), 2);
    /// assert_eq!(vec.as_slice(), &[1, 3]);
    ///
    /// // Remove element from the end
    /// assert_eq!(vec.remove(1), 3);
    /// assert_eq!(vec.as_slice(), &[1]);
    /// # }
    pub fn remove(&mut self, index: usize) -> T {
        assert!(index < self.len);
        let ret = self.memory[index];
        self.len -= 1;
        for i in index..self.len {
            self.memory[i] = self.memory[i + 1];
        }
        ret
    }

    /// Appends an element to the back of the vector.
    ///
    /// # Example
    ///
    /// ```
    /// # #[macro_use] extern crate fixedvec;
    /// # use fixedvec::FixedVec;
    /// # fn main() {
    /// let mut space = alloc_stack!([u8; 3]);
    /// let mut vec = FixedVec::new(&mut space);
    ///
    /// // Pushing appends to the end of the vector
    /// vec.push(1).unwrap();
    /// vec.push(2).unwrap();
    /// vec.push(3).unwrap();
    /// assert_eq!(vec.as_slice(), &[1, 2, 3]);
    ///
    /// // Attempting to push a full vector results in an error
    /// assert!(vec.push(4).is_err());
    /// # }
    /// ```
    pub fn push(&mut self, value: T) -> Result<()> {
        if self.available() >= 1 {
            self.memory[self.len] = value;
            self.len += 1;
            Ok(())
        } else {
            Err(ErrorKind::NoSpace)
        }
    }

    /// Removes the last element from the vector and returns it, or `None` if
    /// the vector is empty
    ///
    /// # Example
    ///
    /// ```
    /// # #[macro_use] extern crate fixedvec;
    /// # use fixedvec::FixedVec;
    /// # fn main() {
    /// let mut space = alloc_stack!([u8; 16]);
    /// let mut vec = FixedVec::new(&mut space);
    /// vec.push_all(&[1, 2]).unwrap();
    /// assert_eq!(vec.pop(), Some(2));
    /// assert_eq!(vec.pop(), Some(1));
    /// assert_eq!(vec.pop(), None);
    /// # }
    /// ```
    pub fn pop(&mut self) -> Option<T> {
        if self.len > 0 {
            self.len -= 1;
            Some(self.memory[self.len])
        } else {
            None
        }
    }

    /// Copies all elements from slice `other` to this vector.
    ///
    /// # Example
    ///
    /// ```
    /// # #[macro_use] extern crate fixedvec;
    /// # use fixedvec::FixedVec;
    /// # fn main() {
    /// let mut space = alloc_stack!([u8; 5]);
    /// let mut vec = FixedVec::new(&mut space);
    ///
    /// // All elements are pushed to vector
    /// vec.push_all(&[1, 2, 3, 4]).unwrap();
    /// assert_eq!(vec.as_slice(), &[1, 2, 3, 4]);
    ///
    /// // If there is insufficient space, NO values are pushed
    /// assert!(vec.push_all(&[5, 6, 7]).is_err());
    /// assert_eq!(vec.as_slice(), &[1, 2, 3, 4]);
    /// # }
    /// ```
    pub fn push_all(&mut self, other: &[T]) -> Result<()> {
        if other.len() > self.available() {
            Err(ErrorKind::NoSpace)
        } else {
            for i in 0..other.len() {
                self.memory[self.len] = other[i];
                self.len += 1;
            }
            Ok(())
        }
    }

    /// Clears the vector, removing all values.
    ///
    /// # Example
    ///
    /// ```
    /// # #[macro_use] extern crate fixedvec;
    /// # use fixedvec::FixedVec;
    /// # fn main() {
    /// let mut space = alloc_stack!([u8; 10]);
    /// let mut vec = FixedVec::new(&mut space);
    /// vec.push_all(&[1, 2, 3]).unwrap();
    /// assert_eq!(vec.len(), 3);
    /// vec.clear();
    /// assert_eq!(vec.len(), 0);
    /// # }
    /// ```
    pub fn clear(&mut self) {
        self.len = 0
    }

    /// Applies the function `f` to all elements in the vector, mutating the
    /// vector in place.
    ///
    /// # Example
    ///
    /// ```
    /// # #[macro_use] extern crate fixedvec;
    /// # use fixedvec::FixedVec;
    /// # fn main() {
    /// let mut space = alloc_stack!([u8; 10]);
    /// let mut vec = FixedVec::new(&mut space);
    ///
    /// vec.push_all(&[1, 2, 3]).unwrap();
    /// vec.map_in_place(|x: &mut u8| { *x *= 2 });
    /// assert_eq!(vec.as_slice(), &[2, 4, 6]);
    /// # }
    /// ```
    pub fn map_in_place<F>(&mut self, f: F) where F: Fn(&mut T) {
        for i in 0..self.len {
            f(&mut self.memory[i]);
        }
    }

    /// Provides a forward iterator.
    ///
    /// # Example
    ///
    /// ```
    /// # #[macro_use] extern crate fixedvec;
    /// # use fixedvec::FixedVec;
    /// # fn main() {
    /// let mut space = alloc_stack!([u8; 10]);
    /// let mut vec = FixedVec::new(&mut space);
    /// vec.push_all(&[1, 2, 3]).unwrap();
    /// {
    ///     let mut iter = vec.iter();
    ///     assert_eq!(iter.next(), Some(1));
    ///     assert_eq!(iter.next(), Some(2));
    ///     assert_eq!(iter.next(), Some(3));
    ///     assert_eq!(iter.next(), None);
    /// }
    /// # }
    /// ```
    pub fn iter(&'a self) -> Iter<'a, T> {
        Iter{list: self, idx: 0}
    }

    /// Removes an element from anywhere in the vector and returns it,
    /// replacing it with the last element.
    ///
    /// This does not preserve ordering, but is O(1)
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of bounds
    ///
    /// # Example
    ///
    /// ```
    /// # #[macro_use] extern crate fixedvec;
    /// # use fixedvec::FixedVec;
    /// # fn main() {
    /// let mut space = alloc_stack!([u8; 10]);
    /// let mut vec = FixedVec::new(&mut space);
    ///
    /// vec.push_all(&[0, 1, 2, 3]).unwrap();
    /// assert_eq!(vec.swap_remove(1), 1);
    /// assert_eq!(vec.as_slice(), &[0, 3, 2]);
    /// # }
    /// ```
    pub fn swap_remove(&mut self, index: usize) -> T {
        assert!(index < self.len);
        if self.len == 1 {
            self.remove(0)
        } else {
            let removed = self.memory[index];
            self.memory[index] = self.pop().unwrap();
            removed
        }
    }

    /// Resizes the vector in-place so that `len()` is equal to `new_len`.
    ///
    /// New elements (if needed) are cloned from `value`.
    ///
    /// # Panics
    ///
    /// Panics if `new_len` is greater than capacity
    ///
    /// # Example
    ///
    /// ```
    /// # #[macro_use] extern crate fixedvec;
    /// # use fixedvec::FixedVec;
    /// # fn main() {
    /// let mut space = alloc_stack!([u8; 10]);
    /// let mut vec = FixedVec::new(&mut space);
    ///
    /// assert_eq!(vec.len(), 0);
    /// vec.resize(5, 255);
    /// assert_eq!(vec.as_slice(), &[255, 255, 255, 255, 255]);
    /// vec.resize(2, 0);
    /// assert_eq!(vec.as_slice(), &[255, 255]);
    /// # }
    /// ```
    pub fn resize(&mut self, new_len: usize, value: T) {
        assert!(new_len <= self.capacity());
        if new_len <= self.len {
            self.len = new_len;
        } else {
            for i in self.memory[self.len..new_len].iter_mut() {
                *i = Clone::clone(&value);
            }
            self.len = new_len;
        }
    }

    /// Retains only the elements specified by the predicate.
    ///
    /// In other words, remove all elements `e` such that `f(&e)` returns
    /// false. This method operates in-place, in O(N) time, and preserves the
    /// order of the retained elements.
    ///
    /// # Example
    ///
    /// ```
    /// # #[macro_use] extern crate fixedvec;
    /// # use fixedvec::FixedVec;
    /// # fn main() {
    /// let mut space = alloc_stack!([u8; 10]);
    /// let mut vec = FixedVec::new(&mut space);
    ///
    /// vec.push_all(&[1, 2, 3, 4]).unwrap();
    /// vec.retain(|&x| x%2 == 0);
    /// assert_eq!(vec.as_slice(), &[2, 4]);
    /// # }
    /// ```
    pub fn retain<F>(&mut self, f: F) where F: Fn(&T) -> bool {
        let mut head: usize = 0;
        let mut tail: usize = 0;
        loop {
            if head >= self.len { break; }
            if f(&self.memory[head]) {
                self.memory[tail] = self.memory[head];
                tail += 1;
            }
            head += 1;
        }
        self.len = tail;
    }
}

impl<'a, T> FixedVec<'a, T> where T: 'a + Copy + PartialEq<T> {
    /// Removes consecutive repeated elements in the vector in O(N) time.
    ///
    /// If the vector is sorted, this removes all duplicates.
    ///
    /// # Example
    ///
    /// ```
    /// # #[macro_use] extern crate fixedvec;
    /// # use fixedvec::FixedVec;
    /// # fn main() {
    /// let mut space = alloc_stack!([u8; 10]);
    /// let mut vec = FixedVec::new(&mut space);
    /// vec.push_all(&[1, 2, 2, 3, 2]).unwrap();
    /// vec.dedup();
    /// assert_eq!(vec.as_slice(), &[1, 2, 3, 2]);
    /// # }
    /// ```
    pub fn dedup(&mut self) {
        if self.len <= 1 { return; }
        let mut head: usize = 1;
        let mut tail: usize = 0;
        loop {
            if head >= self.len { break; }
            if self.memory[head] != self.memory[tail] {
                tail += 1;
                self.memory[tail] = self.memory[head];
            }
            head += 1;
        }
        self.len = tail + 1;
    }
}

impl<'a, T: 'a + Copy> Iterator for Iter<'a, T> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<T> {
        if self.idx >= self.list.len() {
            return None;
        }
        self.idx += 1;
        Some(self.list[self.idx - 1])
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let nelem = self.list.len() - self.idx;
        (nelem, Some(nelem))
    }
}

impl<'a, T: 'a + Copy> ExactSizeIterator for Iter<'a, T> {}

impl<'a, T: Copy + Hash> Hash for FixedVec<'a, T> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        Hash::hash(&*self.memory, state)
    }
}

impl <'a, A: 'a + Copy> Extend<A> for FixedVec<'a, A> {
    fn extend<T: IntoIterator<Item=A>>(&mut self, iterable: T) {
        if self.available() == 0 { return; }
        for n in iterable {
            self.memory[self.len] = n;
            self.len += 1;
            if self.available() == 0 { break; }
        }
    }
}

impl<'a, T: Copy> ops::Index<usize> for FixedVec<'a, T> {
    type Output = T;

    #[inline]
    fn index(&self, index: usize) -> &T {
        &(self.memory)[index]
    }
}

impl<'a, T: Copy> ops::IndexMut<usize> for FixedVec<'a, T> {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut T{
        &mut (self.memory)[index]
    }
}

impl<'a, T: Copy + PartialEq> PartialEq for FixedVec<'a, T> {
    fn eq(&self, other: &FixedVec<'a, T>) -> bool {
        if self.len() != other.len() { return false; }

        (0..self.len()).all(|i| {
            self[i] == other[i]
        })
    }
}

impl<'a, T: Copy + Eq> Eq for FixedVec<'a, T> { }

#[cfg(test)]
mod test {
    use super::FixedVec;

    #[test]
    fn test_extend() {
        let mut space = alloc_stack!([u8; 16]);
        let mut vec = FixedVec::new(&mut space);
        vec.extend(0..6);
        assert_eq!(&[0, 1, 2, 3, 4, 5], vec.as_slice());
    }
}
