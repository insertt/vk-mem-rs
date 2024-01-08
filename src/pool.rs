use std::ffi::CStr;
use std::mem::MaybeUninit;
use std::sync::Arc;

use crate::ffi;
use crate::Allocation;
use crate::AllocationCreateInfo;
use crate::Allocator;
use crate::PoolCreateInfo;
use spark::{vk, Result};

#[derive(Clone, Copy)]
pub struct PoolHandle(ffi::VmaPool);

/// Represents custom memory pool handle.
pub struct AllocatorPool {
    allocator: Arc<Allocator>,
    pub(crate) pool: PoolHandle,
}
unsafe impl Send for AllocatorPool {}
unsafe impl Sync for AllocatorPool {}

impl Allocator {
    /// Allocates Vulkan device memory and creates `AllocatorPool` object.
    pub fn create_pool(self: &Arc<Self>, create_info: &PoolCreateInfo) -> Result<AllocatorPool> {
        unsafe {
            let mut ffi_pool: ffi::VmaPool = std::mem::zeroed();
            ffi::vmaCreatePool(self.internal, &create_info.inner, &mut ffi_pool).result()?;
            Ok(AllocatorPool {
                pool: PoolHandle(ffi_pool),
                allocator: self.clone(),
            })
        }
    }

    pub fn default_pool(self: &Arc<Self>) -> AllocatorPool {
        AllocatorPool {
            pool: PoolHandle(std::ptr::null_mut()),
            allocator: self.clone(),
        }
    }
}

impl Drop for AllocatorPool {
    fn drop(&mut self) {
        unsafe {
            ffi::vmaDestroyPool(self.allocator.internal, self.pool.0);
        }
    }
}

impl AllocatorPool {
    pub fn set_name(&self, name: Option<&CStr>) {
        if self.pool.0.is_null() {
            return;
        }
        unsafe {
            ffi::vmaSetPoolName(
                self.allocator.internal,
                self.pool.0,
                name.map_or(std::ptr::null(), CStr::as_ptr),
            );
        }
    }
    pub fn name(&self) -> Option<&CStr> {
        if self.pool.0.is_null() {
            return None;
        }
        let mut ptr: *const ::std::os::raw::c_char = std::ptr::null();
        unsafe {
            ffi::vmaGetPoolName(self.allocator.internal, self.pool.0, &mut ptr);
            if ptr.is_null() {
                return None;
            }
            Some(CStr::from_ptr(ptr))
        }
    }
    /// Retrieves statistics of existing `AllocatorPool` object.
    pub fn get_statistics(&self) -> Result<ffi::VmaStatistics> {
        unsafe {
            let mut pool_stats: ffi::VmaStatistics = std::mem::zeroed();
            ffi::vmaGetPoolStatistics(self.allocator.internal, self.pool.0, &mut pool_stats);
            Ok(pool_stats)
        }
    }

    /// Retrieves statistics of existing `AllocatorPool` object.
    pub fn calculate_statistics(&self) -> Result<ffi::VmaDetailedStatistics> {
        unsafe {
            let mut pool_stats: ffi::VmaDetailedStatistics = std::mem::zeroed();
            ffi::vmaCalculatePoolStatistics(self.allocator.internal, self.pool.0, &mut pool_stats);
            Ok(pool_stats)
        }
    }

    /// Checks magic number in margins around all allocations in given memory pool in search for corruptions.
    ///
    /// Corruption detection is enabled only when `VMA_DEBUG_DETECT_CORRUPTION` macro is defined to nonzero,
    /// `VMA_DEBUG_MARGIN` is defined to nonzero and the pool is created in memory type that is
    /// `spark::vk::MemoryPropertyFlags::HOST_VISIBLE` and `spark::vk::MemoryPropertyFlags::HOST_COHERENT`.
    ///
    /// Possible error values:
    ///
    /// - `spark::vk::Result::ERROR_FEATURE_NOT_PRESENT` - corruption detection is not enabled for specified pool.
    /// - `spark::vk::Result::ERROR_VALIDATION_FAILED_EXT` - corruption detection has been performed and found memory corruptions around one of the allocations.
    ///   `VMA_ASSERT` is also fired in that case.
    /// - Other value: Error returned by Vulkan, e.g. memory mapping failure.
    pub fn check_corruption(&self) -> Result<()> {
        unsafe { ffi::vmaCheckPoolCorruption(self.allocator.internal, self.pool.0).result() }
    }
}

pub trait Alloc {
    fn allocator(&self) -> &Allocator;
    fn pool(&self) -> PoolHandle;
    /// Helps to find memory type index, given memory type bits and allocation info.
    ///
    /// This algorithm tries to find a memory type that:
    ///
    /// - Is allowed by memory type bits.
    /// - Contains all the flags from `allocation_info.required_flags`.
    /// - Matches intended usage.
    /// - Has as many flags from `allocation_info.preferred_flags` as possible.
    ///
    /// Returns spark::vk::Result::ERROR_FEATURE_NOT_PRESENT if not found. Receiving such a result
    /// from this function or any other allocating function probably means that your
    /// device doesn't support any memory type with requested features for the specific
    /// type of resource you want to use it for. Please check parameters of your
    /// resource, like image layout (OPTIMAL versus LINEAR) or mip level count.
    unsafe fn find_memory_type_index(
        &self,
        memory_type_bits: u32,
        allocation_info: &AllocationCreateInfo,
    ) -> Result<u32> {
        let mut memory_type_index: u32 = 0;
        let mut allocation_info: ffi::VmaAllocationCreateInfo = allocation_info.into();
        allocation_info.pool = self.pool().0;
        ffi::vmaFindMemoryTypeIndex(
            self.allocator().internal,
            memory_type_bits,
            &allocation_info,
            &mut memory_type_index,
        )
        .result()?;

        Ok(memory_type_index)
    }

    /// Helps to find memory type index, given buffer info and allocation info.
    ///
    /// It can be useful e.g. to determine value to be used as `AllocatorPoolCreateInfo::memory_type_index`.
    /// It internally creates a temporary, dummy buffer that never has memory bound.
    /// It is just a convenience function, equivalent to calling:
    ///
    /// - `spark::vk::Device::create_buffer`
    /// - `spark::vk::Device::get_buffer_memory_requirements`
    /// - `Allocator::find_memory_type_index`
    /// - `spark::vk::Device::destroy_buffer`
    unsafe fn find_memory_type_index_for_buffer_info(
        &self,
        buffer_info: &spark::vk::BufferCreateInfo,
        allocation_info: &AllocationCreateInfo,
    ) -> Result<u32> {
        let mut allocation_info: ffi::VmaAllocationCreateInfo = allocation_info.into();
        allocation_info.pool = self.pool().0;
        let mut memory_type_index: u32 = 0;
        ffi::vmaFindMemoryTypeIndexForBufferInfo(
            self.allocator().internal,
            buffer_info,
            &allocation_info,
            &mut memory_type_index,
        )
        .result()?;

        Ok(memory_type_index)
    }

    /// Helps to find memory type index, given image info and allocation info.
    ///
    /// It can be useful e.g. to determine value to be used as `AllocatorPoolCreateInfo::memory_type_index`.
    /// It internally creates a temporary, dummy image that never has memory bound.
    /// It is just a convenience function, equivalent to calling:
    ///
    /// - `spark::vk::Device::create_image`
    /// - `spark::vk::Device::get_image_memory_requirements`
    /// - `Allocator::find_memory_type_index`
    /// - `spark::vk::Device::destroy_image`
    unsafe fn find_memory_type_index_for_image_info(
        &self,
        image_info: spark::vk::ImageCreateInfo,
        allocation_info: &AllocationCreateInfo,
    ) -> Result<u32> {
        let mut allocation_info: ffi::VmaAllocationCreateInfo = allocation_info.into();
        allocation_info.pool = self.pool().0;
        let mut memory_type_index: u32 = 0;
        ffi::vmaFindMemoryTypeIndexForImageInfo(
            self.allocator().internal,
            &image_info,
            &allocation_info,
            &mut memory_type_index,
        )
        .result()?;

        Ok(memory_type_index)
    }

    /// General purpose memory allocation.
    ///
    /// You should free the memory using `Allocator::free_memory` or 'Allocator::free_memory_pages'.
    ///
    /// It is recommended to use `Allocator::allocate_memory_for_buffer`, `Allocator::allocate_memory_for_image`,
    /// `Allocator::create_buffer`, `Allocator::create_image` instead whenever possible.
    unsafe fn allocate_memory(
        &self,
        memory_requirements: &spark::vk::MemoryRequirements,
        create_info: &AllocationCreateInfo,
    ) -> Result<Allocation> {
        let mut create_info: ffi::VmaAllocationCreateInfo = create_info.into();
        create_info.pool = self.pool().0;
        let mut allocation: ffi::VmaAllocation = std::mem::zeroed();
        ffi::vmaAllocateMemory(
            self.allocator().internal,
            memory_requirements,
            &create_info,
            &mut allocation,
            std::ptr::null_mut(),
        )
        .result()?;

        Ok(Allocation(allocation))
    }

    /// General purpose memory allocation for multiple allocation objects at once.
    ///
    /// You should free the memory using `Allocator::free_memory` or `Allocator::free_memory_pages`.
    ///
    /// Word "pages" is just a suggestion to use this function to allocate pieces of memory needed for sparse binding.
    /// It is just a general purpose allocation function able to make multiple allocations at once.
    /// It may be internally optimized to be more efficient than calling `Allocator::allocate_memory` `allocations.len()` times.
    ///
    /// All allocations are made using same parameters. All of them are created out of the same memory pool and type.
    unsafe fn allocate_memory_pages(
        &self,
        memory_requirements: &spark::vk::MemoryRequirements,
        create_info: &AllocationCreateInfo,
        allocation_count: usize,
    ) -> Result<Vec<Allocation>> {
        let mut create_info: ffi::VmaAllocationCreateInfo = create_info.into();
        create_info.pool = self.pool().0;
        let mut allocations: Vec<ffi::VmaAllocation> = vec![std::mem::zeroed(); allocation_count];
        ffi::vmaAllocateMemoryPages(
            self.allocator().internal,
            memory_requirements,
            &create_info,
            allocation_count,
            allocations.as_mut_ptr(),
            std::ptr::null_mut(),
        )
        .result()?;

        let allocations: Vec<Allocation> = allocations
            .into_iter()
            .map(|alloc| Allocation(alloc))
            .collect();

        Ok(allocations)
    }

    /// Buffer specialized memory allocation.
    ///
    /// You should free the memory using `Allocator::free_memory` or 'Allocator::free_memory_pages'.
    unsafe fn allocate_memory_for_buffer(
        &self,
        buffer: spark::vk::Buffer,
        create_info: &AllocationCreateInfo,
    ) -> Result<Allocation> {
        let mut create_info: ffi::VmaAllocationCreateInfo = create_info.into();
        create_info.pool = self.pool().0;
        let mut allocation = MaybeUninit::zeroed();
        let mut allocation_info = MaybeUninit::zeroed();
        ffi::vmaAllocateMemoryForBuffer(
            self.allocator().internal,
            buffer,
            &create_info,
            allocation.as_mut_ptr(),
            allocation_info.as_mut_ptr(),
        )
        .result()?;

        Ok(Allocation(allocation.assume_init()))
    }

    /// Image specialized memory allocation.
    ///
    /// You should free the memory using `Allocator::free_memory` or 'Allocator::free_memory_pages'.
    unsafe fn allocate_memory_for_image(
        &self,
        image: spark::vk::Image,
        create_info: &AllocationCreateInfo,
    ) -> Result<Allocation> {
        let mut create_info: ffi::VmaAllocationCreateInfo = create_info.into();
        create_info.pool = self.pool().0;
        let mut allocation = MaybeUninit::zeroed();
        ffi::vmaAllocateMemoryForImage(
            self.allocator().internal,
            image,
            &create_info,
            allocation.as_mut_ptr(),
            std::ptr::null_mut(),
        )
        .result()?;

        Ok(Allocation(allocation.assume_init()))
    }

    /// This function automatically creates a buffer, allocates appropriate memory
    /// for it, and binds the buffer with the memory.
    ///
    /// If the function succeeded, you must destroy both buffer and allocation when you
    /// no longer need them using either convenience function `Allocator::destroy_buffer` or
    /// separately, using `ash::Device::destroy_buffer` and `Allocator::free_memory`.
    ///
    /// If `AllocatorCreateFlags::KHR_DEDICATED_ALLOCATION` flag was used,
    /// VK_KHR_dedicated_allocation extension is used internally to query driver whether
    /// it requires or prefers the new buffer to have dedicated allocation. If yes,
    /// and if dedicated allocation is possible (AllocationCreateInfo::pool is null
    /// and `AllocationCreateFlags::NEVER_ALLOCATE` is not used), it creates dedicated
    /// allocation for this buffer, just like when using `AllocationCreateFlags::DEDICATED_MEMORY`.
    unsafe fn create_buffer(
        &self,
        buffer_info: &spark::vk::BufferCreateInfo,
        create_info: &AllocationCreateInfo,
    ) -> Result<(spark::vk::Buffer, Allocation)> {
        let mut create_info: ffi::VmaAllocationCreateInfo = create_info.into();
        create_info.pool = self.pool().0;
        let mut buffer = MaybeUninit::zeroed();
        let mut allocation = MaybeUninit::zeroed();
        ffi::vmaCreateBuffer(
            self.allocator().internal,
            &*buffer_info,
            &create_info,
            buffer.as_mut_ptr(),
            allocation.as_mut_ptr(),
            std::ptr::null_mut(),
        )
        .result()?;

        Ok((buffer.assume_init(), Allocation(allocation.assume_init())))
    }
    /// brief Creates a buffer with additional minimum alignment.
    ///
    /// Similar to vmaCreateBuffer() but provides additional parameter `minAlignment` which allows to specify custom,
    /// minimum alignment to be used when placing the buffer inside a larger memory block, which may be needed e.g.
    /// for interop with OpenGL.
    unsafe fn create_buffer_with_alignment(
        &self,
        buffer_info: &spark::vk::BufferCreateInfo,
        create_info: &AllocationCreateInfo,
        min_alignment: vk::DeviceSize,
    ) -> Result<(spark::vk::Buffer, Allocation)> {
        let mut create_info: ffi::VmaAllocationCreateInfo = create_info.into();
        create_info.pool = self.pool().0;
        let mut buffer = MaybeUninit::zeroed();
        let mut allocation = MaybeUninit::zeroed();
        ffi::vmaCreateBufferWithAlignment(
            self.allocator().internal,
            &*buffer_info,
            &create_info,
            min_alignment,
            buffer.as_mut_ptr(),
            allocation.as_mut_ptr(),
            std::ptr::null_mut(),
        )
        .result()?;

        Ok((buffer.assume_init(), Allocation(allocation.assume_init())))
    }
    /// This function automatically creates an image, allocates appropriate memory
    /// for it, and binds the image with the memory.
    ///
    /// If the function succeeded, you must destroy both image and allocation when you
    /// no longer need them using either convenience function `Allocator::destroy_image` or
    /// separately, using `ash::Device::destroy_image` and `Allocator::free_memory`.
    ///
    /// If `AllocatorCreateFlags::KHR_DEDICATED_ALLOCATION` flag was used,
    /// `VK_KHR_dedicated_allocation extension` is used internally to query driver whether
    /// it requires or prefers the new image to have dedicated allocation. If yes,
    /// and if dedicated allocation is possible (AllocationCreateInfo::pool is null
    /// and `AllocationCreateFlags::NEVER_ALLOCATE` is not used), it creates dedicated
    /// allocation for this image, just like when using `AllocationCreateFlags::DEDICATED_MEMORY`.
    ///
    /// If `VK_ERROR_VALIDAITON_FAILED_EXT` is returned, VMA may have encountered a problem
    /// that is not caught by the validation layers. One example is if you try to create a 0x0
    /// image, a panic will occur and `VK_ERROR_VALIDAITON_FAILED_EXT` is thrown.
    unsafe fn create_image(
        &self,
        image_info: &spark::vk::ImageCreateInfo,
        create_info: &AllocationCreateInfo,
    ) -> Result<(spark::vk::Image, Allocation)> {
        let mut create_info: ffi::VmaAllocationCreateInfo = create_info.into();
        create_info.pool = self.pool().0;
        let mut image = MaybeUninit::zeroed();
        let mut allocation = MaybeUninit::zeroed();
        ffi::vmaCreateImage(
            self.allocator().internal,
            &*image_info,
            &create_info,
            image.as_mut_ptr(),
            allocation.as_mut_ptr(),
            std::ptr::null_mut(),
        );

        Ok((image.assume_init(), Allocation(allocation.assume_init())))
    }
}

impl Alloc for AllocatorPool {
    fn allocator(&self) -> &Allocator {
        self.allocator.as_ref()
    }

    fn pool(&self) -> PoolHandle {
        self.pool
    }
}
impl Alloc for Allocator {
    fn allocator(&self) -> &Allocator {
        self
    }

    fn pool(&self) -> PoolHandle {
        PoolHandle(std::ptr::null_mut())
    }
}
