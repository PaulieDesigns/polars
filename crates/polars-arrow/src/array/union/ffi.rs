use polars_error::PolarsResult;

use super::super::Array;
use super::super::ffi::ToFfi;
use super::UnionArray;
use crate::array::FromFfi;
use crate::ffi;

unsafe impl ToFfi for UnionArray {
    fn buffers(&self) -> Vec<Option<*const u8>> {
        if let Some(offsets) = &self.offsets {
            vec![
                Some(self.types.storage_ptr().cast::<u8>()),
                Some(offsets.storage_ptr().cast::<u8>()),
            ]
        } else {
            vec![Some(self.types.storage_ptr().cast::<u8>())]
        }
    }

    fn children(&self) -> Vec<Box<dyn Array>> {
        self.fields.clone()
    }

    fn offset(&self) -> Option<usize> {
        Some(self.types.offset())
    }

    fn to_ffi_aligned(&self) -> Self {
        self.clone()
    }
}

impl<A: ffi::ArrowArrayRef> FromFfi<A> for UnionArray {
    unsafe fn try_from_ffi(array: A) -> PolarsResult<Self> {
        let dtype = array.dtype().clone();
        let fields = Self::get_fields(&dtype);

        let mut types = unsafe { array.buffer::<i8>(0) }?;
        let offsets = if Self::is_sparse(&dtype) {
            None
        } else {
            Some(unsafe { array.buffer::<i32>(1) }?)
        };

        let length = array.array().len();
        let offset = array.array().offset();
        let fields = (0..fields.len())
            .map(|index| {
                let child = array.child(index)?;
                ffi::try_from(child)
            })
            .collect::<PolarsResult<Vec<Box<dyn Array>>>>()?;

        if offset > 0 {
            types.slice(offset, length);
        };

        Self::try_new(dtype, types, fields, offsets)
    }
}
