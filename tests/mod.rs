extern crate ash;
extern crate vk_mem;

use ash::{extensions::ext::DebugUtils, vk};
use std::{os::raw::c_void, sync::Arc};
use vk_mem::Alloc;

fn extension_names() -> Vec<*const i8> {
    vec![DebugUtils::name().as_ptr()]
}

unsafe extern "system" fn vulkan_debug_callback(
    _message_severity: spark::vk::DebugUtilsMessageSeverityFlagsEXT,
    _message_types: spark::vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const spark::vk::DebugUtilsMessengerCallbackDataEXT,
    _p_user_data: *mut c_void,
) -> spark::vk::Bool32 {
    let p_callback_data = &*p_callback_data;
    println!(
        "{:?}",
        ::std::ffi::CStr::from_ptr(p_callback_data.p_message)
    );
    spark::vk::FALSE
}

pub struct TestHarness {
    pub entry: ash::Entry,
    pub instance: ash::Instance,
    pub device: ash::Device,
    pub physical_device: spark::vk::PhysicalDevice,
    pub debug_callback: spark::vk::DebugUtilsMessengerEXT,
    pub debug_report_loader: ash::extensions::ext::DebugUtils,
}

impl Drop for TestHarness {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();
            self.device.destroy_device(None);
            self.debug_report_loader
                .destroy_debug_utils_messenger(self.debug_callback, None);
            self.instance.destroy_instance(None);
        }
    }
}
impl TestHarness {
    pub fn new() -> Self {
        let app_name = ::std::ffi::CString::new("vk-mem testing").unwrap();
        let app_info = spark::vk::ApplicationInfo::builder()
            .application_name(&app_name)
            .application_version(0)
            .engine_name(&app_name)
            .engine_version(0)
            .api_version(spark::vk::make_api_version(0, 1, 3, 0));

        let layer_names = [::std::ffi::CString::new("VK_LAYER_KHRONOS_validation").unwrap()];
        let layers_names_raw: Vec<*const i8> = layer_names
            .iter()
            .map(|raw_name| raw_name.as_ptr())
            .collect();

        let extension_names_raw = extension_names();
        let create_info = spark::vk::InstanceCreateInfo::builder()
            .application_info(&app_info)
            .enabled_layer_names(&layers_names_raw)
            .enabled_extension_names(&extension_names_raw);

        let entry = unsafe { ash::Entry::load().unwrap() };
        let instance: ash::Instance = unsafe {
            entry
                .create_instance(&create_info, None)
                .expect("Instance creation error")
        };

        let debug_info = spark::vk::DebugUtilsMessengerCreateInfoEXT::builder()
            .message_severity(
                spark::vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                    | spark::vk::DebugUtilsMessageSeverityFlagsEXT::WARNING,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION,
            )
            .pfn_user_callback(Some(vulkan_debug_callback));

        let debug_report_loader = DebugUtils::new(&entry, &instance);
        let debug_callback = unsafe {
            debug_report_loader
                .create_debug_utils_messenger(&debug_info, None)
                .unwrap()
        };

        let physical_devices = unsafe {
            instance
                .enumerate_physical_devices()
                .expect("Physical device error")
        };

        let (physical_device, queue_family_index) = unsafe {
            physical_devices
                .iter()
                .map(|physical_device| {
                    instance
                        .get_physical_device_queue_family_properties(*physical_device)
                        .iter()
                        .enumerate()
                        .filter_map(|(index, _)| Some((*physical_device, index)))
                        .nth(0)
                })
                .filter_map(|v| v)
                .nth(0)
                .expect("Couldn't find suitable device.")
        };

        let priorities = [1.0];

        let queue_info = [spark::vk::DeviceQueueCreateInfo::builder()
            .queue_family_index(queue_family_index as u32)
            .queue_priorities(&priorities)
            .build()];

        let device_create_info =
            spark::vk::DeviceCreateInfo::builder().queue_create_infos(&queue_info);

        let device: ash::Device = unsafe {
            instance
                .create_device(physical_device, &device_create_info, None)
                .unwrap()
        };

        TestHarness {
            entry,
            instance,
            device,
            physical_device,
            debug_report_loader,
            debug_callback,
        }
    }

    pub fn create_allocator(&self) -> vk_mem::Allocator {
        let create_info =
            vk_mem::AllocatorCreateInfo::new(&self.instance, &self.device, self.physical_device);
        vk_mem::Allocator::new(create_info).unwrap()
    }
}

#[test]
fn create_harness() {
    let _ = TestHarness::new();
}

#[test]
fn create_allocator() {
    let harness = TestHarness::new();
    let _ = harness.create_allocator();
}

#[test]
fn create_gpu_buffer() {
    let harness = TestHarness::new();
    let allocator = harness.create_allocator();
    let allocation_info = vk_mem::AllocationCreateInfo {
        usage: vk_mem::MemoryUsage::Auto,
        ..Default::default()
    };

    unsafe {
        let (buffer, allocation) = allocator
            .create_buffer(
                &spark::vk::BufferCreateInfo::builder()
                    .size(16 * 1024)
                    .usage(
                        spark::vk::BufferUsageFlags::VERTEX_BUFFER
                            | spark::vk::BufferUsageFlags::TRANSFER_DST,
                    )
                    .build(),
                &allocation_info,
            )
            .unwrap();
        let allocation_info = allocator.get_allocation_info(&allocation).unwrap();
        assert_eq!(allocation_info.mapped_data, std::ptr::null_mut());
        allocator.destroy_buffer(buffer, allocation);
    }
}

#[test]
fn create_cpu_buffer_preferred() {
    let harness = TestHarness::new();
    let allocator = harness.create_allocator();
    let allocation_info = vk_mem::AllocationCreateInfo {
        required_flags: spark::vk::MemoryPropertyFlags::HOST_VISIBLE,
        preferred_flags: spark::vk::MemoryPropertyFlags::HOST_COHERENT
            | spark::vk::MemoryPropertyFlags::HOST_CACHED,
        flags: vk_mem::AllocationCreateFlags::MAPPED,
        ..Default::default()
    };
    unsafe {
        let (buffer, allocation) = allocator
            .create_buffer(
                &spark::vk::BufferCreateInfo::builder()
                    .size(16 * 1024)
                    .usage(
                        spark::vk::BufferUsageFlags::VERTEX_BUFFER
                            | spark::vk::BufferUsageFlags::TRANSFER_DST,
                    )
                    .build(),
                &allocation_info,
            )
            .unwrap();
        let allocation_info = allocator.get_allocation_info(&allocation).unwrap();
        assert_ne!(allocation_info.mapped_data, std::ptr::null_mut());
        allocator.destroy_buffer(buffer, allocation);
    }
}

#[test]
fn create_gpu_buffer_pool() {
    let harness = TestHarness::new();
    let allocator = harness.create_allocator();
    let allocator = Arc::new(allocator);

    let buffer_info = spark::vk::BufferCreateInfo::builder()
        .size(16 * 1024)
        .usage(spark::vk::BufferUsageFlags::UNIFORM_BUFFER | spark::vk::BufferUsageFlags::TRANSFER_DST)
        .build();

    let allocation_info = vk_mem::AllocationCreateInfo {
        required_flags: spark::vk::MemoryPropertyFlags::HOST_VISIBLE,
        preferred_flags: spark::vk::MemoryPropertyFlags::HOST_COHERENT
            | spark::vk::MemoryPropertyFlags::HOST_CACHED,
        flags: vk_mem::AllocationCreateFlags::MAPPED,

        ..Default::default()
    };
    unsafe {
        let memory_type_index = allocator
            .find_memory_type_index_for_buffer_info(&buffer_info, &allocation_info)
            .unwrap();

        // Create a pool that can have at most 2 blocks, 128 MiB each.
        let pool_info = vk_mem::PoolCreateInfo::new()
            .memory_type_index(memory_type_index)
            .block_size(128 * 1024 * 1024)
            .max_block_count(2);

        let pool = allocator.create_pool(&pool_info).unwrap();

        let (buffer, allocation) = pool.create_buffer(&buffer_info, &allocation_info).unwrap();
        let allocation_info = allocator.get_allocation_info(&allocation).unwrap();
        assert_ne!(allocation_info.mapped_data, std::ptr::null_mut());
        allocator.destroy_buffer(buffer, allocation);
    }
}

#[test]
fn test_gpu_stats() {
    let harness = TestHarness::new();
    let allocator = harness.create_allocator();
    let allocation_info = vk_mem::AllocationCreateInfo {
        usage: vk_mem::MemoryUsage::Auto,
        ..Default::default()
    };

    unsafe {
        let stats_1 = allocator.calculate_statistics().unwrap();
        assert_eq!(stats_1.total.statistics.blockCount, 0);
        assert_eq!(stats_1.total.statistics.allocationCount, 0);
        assert_eq!(stats_1.total.statistics.allocationBytes, 0);

        let (buffer, allocation) = allocator
            .create_buffer(
                &spark::vk::BufferCreateInfo::builder()
                    .size(16 * 1024)
                    .usage(
                        spark::vk::BufferUsageFlags::VERTEX_BUFFER
                            | spark::vk::BufferUsageFlags::TRANSFER_DST,
                    )
                    .build(),
                &allocation_info,
            )
            .unwrap();

        let stats_2 = allocator.calculate_statistics().unwrap();
        assert_eq!(stats_2.total.statistics.blockCount, 1);
        assert_eq!(stats_2.total.statistics.allocationCount, 1);
        assert_eq!(stats_2.total.statistics.allocationBytes, 16 * 1024);

        allocator.destroy_buffer(buffer, allocation);

        let stats_3 = allocator.calculate_statistics().unwrap();
        assert_eq!(stats_3.total.statistics.blockCount, 1);
        assert_eq!(stats_3.total.statistics.allocationCount, 0);
        assert_eq!(stats_3.total.statistics.allocationBytes, 0);
    }
}

#[test]
fn create_virtual_block() {
    let create_info = vk_mem::VirtualBlockCreateInfo::new().size(16 * 1024 * 1024); // 16MB block
    let _virtual_block = vk_mem::VirtualBlock::new(create_info)
        .expect("Couldn't create VirtualBlock");
}

#[test]
fn virtual_allocate_and_free() {
    let create_info = vk_mem::VirtualBlockCreateInfo::new().size(16 * 1024 * 1024); // 16MB block
    let virtual_block = vk_mem::VirtualBlock::new(create_info)
        .expect("Couldn't create VirtualBlock");

    let allocation_info = vk_mem::VirtualAllocationCreateInfo {
        size: 8 * 1024 * 1024,
        alignment: 0,
        user_data: 0,
        flags: vk_mem::VirtualAllocationCreateFlags::empty(),
    };

    // Fully allocate the VirtualBlock and then free both allocations
    unsafe {
        let (virtual_alloc_0, offset_0) = virtual_block.allocate(allocation_info).unwrap();
        let (virtual_alloc_1, offset_1) = virtual_block.allocate(allocation_info).unwrap();
        assert_ne!(offset_0, offset_1);
        virtual_block.free(virtual_alloc_0);
        virtual_block.free(virtual_alloc_1);
    }

    // Fully allocate it again and then clear it
    unsafe {
        let (_virtual_alloc_0, offset_0) = virtual_block.allocate(allocation_info).unwrap();
        let (_virtual_alloc_1, offset_1) = virtual_block.allocate(allocation_info).unwrap();
        assert_ne!(offset_0, offset_1);
        virtual_block.clear();
    }

    // VMA should trigger an assert when the VirtualBlock is dropped, if any
    // allocations have not been freed, or the block not cleared instead
}

#[test]
fn virtual_allocation_user_data() {
    let create_info = vk_mem::VirtualBlockCreateInfo::new().size(16 * 1024 * 1024); // 16MB block
    let virtual_block = vk_mem::VirtualBlock::new(create_info)
        .expect("Couldn't create VirtualBlock");

    let user_data = Box::new(vec![12, 34, 56, 78, 90]);
    let allocation_info = vk_mem::VirtualAllocationCreateInfo {
        size: 8 * 1024 * 1024,
        alignment: 0,
        user_data: user_data.as_ptr() as usize,
        flags: vk_mem::VirtualAllocationCreateFlags::empty(),
    };

    unsafe {
        let (virtual_alloc_0, _) = virtual_block.allocate(allocation_info).unwrap();
        let queried_info = virtual_block.get_allocation_info(&virtual_alloc_0)
            .expect("Couldn't get VirtualAllocationInfo from VirtualBlock");
        let queried_user_data = std::slice::from_raw_parts(queried_info.user_data as *const i32, 5);
        assert_eq!(queried_user_data, &*user_data);
        virtual_block.free(virtual_alloc_0);
    }
}

#[test]
fn virtual_block_out_of_space() {
    let create_info = vk_mem::VirtualBlockCreateInfo::new().size(16 * 1024 * 1024); // 16MB block
    let virtual_block = vk_mem::VirtualBlock::new(create_info)
        .expect("Couldn't create VirtualBlock");

    let allocation_info = vk_mem::VirtualAllocationCreateInfo {
        size: 16 * 1024 * 1024 + 1,
        alignment: 0,
        user_data: 0,
        flags: vk_mem::VirtualAllocationCreateFlags::empty(),
    };

    unsafe {
        match virtual_block.allocate(allocation_info) {
            Ok(_) => panic!("Created VirtualAllocation larger than VirtualBlock"),
            Err(spark::vk::Result::ERROR_OUT_OF_DEVICE_MEMORY) => {},
            Err(_) => panic!("Unexpected VirtualBlock error"),
        }
    }
}
