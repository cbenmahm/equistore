#![allow(clippy::default_trait_access, clippy::module_name_repetitions)]

use std::ffi::CString;
use std::collections::{BTreeSet, HashMap};
use std::collections::hash_map::Entry;

use smallvec::SmallVec;

use crate::Error;
use crate::utils::ConstCString;

/// A single value inside a label. This is represented as a 32-bit signed
/// integer, with a couple of helper function to get its value as usize/isize.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct LabelValue(i32);

impl PartialEq<i32> for LabelValue {
    fn eq(&self, other: &i32) -> bool {
        self.0 == *other
    }
}

impl PartialEq<LabelValue> for i32 {
    fn eq(&self, other: &LabelValue) -> bool {
        *self == other.0
    }
}

impl std::fmt::Debug for LabelValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Display for LabelValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
impl From<u32> for LabelValue {
    fn from(value: u32) -> LabelValue {
        assert!(value < i32::MAX as u32);
        LabelValue(value as i32)
    }
}

impl From<i32> for LabelValue {
    fn from(value: i32) -> LabelValue {
        LabelValue(value)
    }
}

#[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
impl From<usize> for LabelValue {
    fn from(value: usize) -> LabelValue {
        assert!(value < i32::MAX as usize);
        LabelValue(value as i32)
    }
}

#[allow(clippy::cast_possible_truncation)]
impl From<isize> for LabelValue {
    fn from(value: isize) -> LabelValue {
        assert!(value < i32::MAX as isize && value > i32::MIN as isize);
        LabelValue(value as i32)
    }
}

impl LabelValue {
    /// Create a `LabelValue` with the given `value`
    pub fn new(value: i32) -> LabelValue {
        LabelValue(value)
    }

    /// Get the integer value of this `LabelValue` as a usize
    #[allow(clippy::cast_sign_loss)]
    pub fn usize(self) -> usize {
        debug_assert!(self.0 >= 0);
        self.0 as usize
    }

    /// Get the integer value of this `LabelValue` as an isize
    pub fn isize(self) -> isize {
        self.0 as isize
    }

    /// Get the integer value of this `LabelValue` as an i32
    pub fn i32(self) -> i32 {
        self.0
    }
}

/// Builder for `Labels`, this should be used to construct `Labels`.
pub struct LabelsBuilder {
    // cf `Labels` for the documentation of the fields
    names: Vec<String>,
    values: Vec<LabelValue>,
    positions: HashMap<SmallVec<[LabelValue; 4]>, usize, ahash::RandomState>,
}

impl LabelsBuilder {
    /// Create a new empty `LabelsBuilder` with the given `names`
    pub fn new(names: Vec<&str>) -> LabelsBuilder {
        for name in &names {
            assert!(is_valid_label_name(name), "all labels names must be valid identifiers, '{}' is not", name);
        }

        let n_unique_names = names.iter().collect::<BTreeSet<_>>().len();
        assert!(n_unique_names == names.len(), "invalid labels: the same name is used multiple times");

        LabelsBuilder {
            names: names.into_iter().map(|s| s.into()).collect(),
            values: Vec::new(),
            positions: Default::default(),
        }
    }

    /// Reserve space for `additional` other entries in the labels.
    pub fn reserve(&mut self, additional: usize) {
        self.values.reserve(additional * self.names.len());
        self.positions.reserve(additional);
    }

    /// Get the number of labels in a single value
    pub fn size(&self) -> usize {
        self.names.len()
    }

    /// Add a single `entry` to this set of labels.
    ///
    /// This function will panic when attempting to add the same `label` more
    /// than once.
    pub fn add<T>(&mut self, entry: &[T]) -> Result<(), Error> where T: Copy + Into<LabelValue> {
        assert_eq!(
            self.size(), entry.len(),
            "wrong size for added label: got {}, but expected {}",
            entry.len(), self.size()
        );

        let entry = entry.iter().copied().map(Into::into).collect::<SmallVec<_>>();
        self.values.extend(&entry);

        let new_position = self.positions.len();
        match self.positions.entry(entry) {
            Entry::Occupied(entry) => {
                let values_display = entry.key().iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", ");
                return Err(Error::InvalidParameter(format!(
                    "can not have the same label value multiple time: [{}] is already present at position {}",
                    values_display, entry.get()
                )));
            },
            Entry::Vacant(entry) => {
                entry.insert(new_position);
            }
        }

        Ok(())
    }

    /// Finish building the `Labels`
    pub fn finish(self) -> Labels {
        if self.names.is_empty() {
            assert!(self.values.is_empty());
            return Labels {
                names: Vec::new(),
                values: Vec::new(),
                positions: Default::default(),
            }
        }

        let names = self.names.into_iter()
            .map(|s| ConstCString::new(CString::new(s).expect("invalid C string")))
            .collect::<Vec<_>>();

        return Labels {
            names: names,
            values: self.values,
            positions: self.positions,
        };
    }
}

/// Check if the given name is a valid identifier, to be used as a
/// column name in `Labels`.
pub fn is_valid_label_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    for (i, c) in name.chars().enumerate() {
        if i == 0 && c.is_ascii_digit() {
            return false;
        }

        if !(c.is_ascii_alphanumeric() || c == '_') {
            return false;
        }
    }

    return true;
}

/// A set of labels used to carry metadata associated with a tensor map.
///
/// This is similar to a list of named tuples, but stored as a 2D array of shape
/// `(labels.count(), labels.size())`, with a of set names associated with the
/// columns of this array. Each row/entry in this array is unique, and they are
/// often (but not always) sorted in  lexicographic order.
///
/// The main way to construct a new set of labels is to use a `LabelsBuilder`.
#[derive(Clone, PartialEq, Eq)]
pub struct Labels {
    /// Names of the labels, stored as const C strings for easier integration
    /// with the C API
    names: Vec<ConstCString>,
    /// Values of the labels, as a linearized 2D array in row-major order
    values: Vec<LabelValue>,
    /// Store the position of all the known labels, for faster access later.
    /// This uses `XxHash64` instead of the default hasher in std since
    /// `XxHash64` is much faster and we don't need the cryptographic strength
    /// hash from std.
    positions: HashMap<SmallVec<[LabelValue; 4]>, usize, ahash::RandomState>,
}

impl std::fmt::Debug for Labels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Labels{{")?;
        writeln!(f, "    {}", self.names().join(", "))?;

        let widths = self.names().iter().map(|s| s.len()).collect::<Vec<_>>();
        for values in self {
            write!(f, "    ")?;
            for (value, width) in values.iter().zip(&widths) {
                write!(f, "{:^width$}  ", value.isize(), width=width)?;
            }
            writeln!(f)?;
        }

        writeln!(f, "}}")?;
        Ok(())
    }
}

impl Labels {
    /// Get the number of entries/named values in a single label
    pub fn size(&self) -> usize {
        self.names.len()
    }

    /// Get the names of the entries/columns in this set of labels
    pub fn names(&self) -> Vec<&str> {
        self.names.iter().map(|s| s.as_str()).collect()
    }

    /// Get the names of the entries/columns in this set of labels as
    /// C-compatible (null terminated) strings
    pub fn c_names(&self) -> &[ConstCString] {
        &self.names
    }

    /// Get the total number of entries in this set of labels
    pub fn count(&self) -> usize {
        if self.size() == 0 {
            return 0;
        } else {
            return self.values.len() / self.size();
        }
    }

    /// Check if this set of Labels is empty (contains no entry)
    pub fn is_empty(&self) -> bool {
        self.count() == 0
    }

    /// Check whether the given `label` is part of this set of labels
    pub fn contains(&self, label: &[LabelValue]) -> bool {
        self.positions.contains_key(label)
    }

    /// Get the position (i.e. row index) of the given label in the full labels
    /// array, or None.
    pub fn position(&self, value: &[LabelValue]) -> Option<usize> {
        assert!(value.len() == self.size(), "invalid size of index in Labels::position");

        self.positions.get(value).copied()
    }

    /// Iterate over the entries in this set of labels
    pub fn iter(&self) -> Iter {
        debug_assert!(self.values.len() % self.names.len() == 0);
        return Iter {
            chunks: self.values.chunks_exact(self.names.len())
        };
    }
}

/// iterator over `Labels` entries
pub struct Iter<'a> {
    chunks: std::slice::ChunksExact<'a, LabelValue>,
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a [LabelValue];

    fn next(&mut self) -> Option<Self::Item> {
        self.chunks.next()
    }
}

impl<'a> ExactSizeIterator for Iter<'a> {
    fn len(&self) -> usize {
        self.chunks.len()
    }
}

impl<'a> IntoIterator for &'a Labels {
    type IntoIter = Iter<'a>;
    type Item = &'a [LabelValue];
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl std::ops::Index<usize> for Labels {
    type Output = [LabelValue];
    fn index(&self, i: usize) -> &[LabelValue] {
        let start = i * self.size();
        let stop = (i + 1) * self.size();
        &self.values[start..stop]
    }
}
