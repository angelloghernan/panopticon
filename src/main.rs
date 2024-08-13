use std::ffi::OsStr;

fn main() {
    // read env variables that were set in build script
    // let uefi_path = env!("UEFI_PATH");
    let bios_path = env!("BIOS_PATH");

    // choose whether to start the UEFI or BIOS image
    // let uefi = false;

    let mut cmd = std::process::Command::new("qemu-system-x86_64");
    // if uefi {
    // cmd.arg("-bios").arg(ovmf_prebuilt::ovmf_pure_efi());
    // cmd.arg("-drive").arg(format!("format=raw,file={uefi_path}"));
    // } else {
    cmd.arg("-device").arg("piix4-ide,bus=pci.0,id=piix4-ide");
    cmd.arg("-drive")
        .arg(format!("file={bios_path},if=none,format=raw,id=bootdisk"));
    cmd.arg("-device")
        .arg("ide-hd,drive=bootdisk,bus=piix4-ide.0");
    cmd.arg("-drive")
        .arg("file=img/disk.img,if=none,format=raw,id=maindisk");
    cmd.arg("-device").arg("ahci,id=ahci");
    cmd.arg("-device").arg("ide-hd,drive=maindisk,bus=ahci.0");
    cmd.arg("-d")
        .arg("trace:ahci_port_write,trace:ahci_check_irq,trace:ahci_port_read,trace:handle_cmd_*");
    // cmd.arg("-d").arg("trace:handle_cmd_*");
    // cmd.arg("-d").arg("trace:ahci_trigger_irq");
    // cmd.arg("-d").arg("int");
    // cmd.arg("-d").arg("trace:execute_ncq_command_*");
    // cmd.arg("-s");
    // cmd.arg("-S");
    // -device piix4-ide,bus=pci.0,id=piix4-ide
    // -drive file=${OBJ_FOLDER}/${OS_IMAGE},if=none,format=raw,id=bootdisk\
    //-device ide-hd,drive=bootdisk,bus=piix4-ide.0 \
    // -drive file=img/disk.img,if=none,format=raw,id=maindisk\
    // -device ahci,id=ahci \
    // -device ide-hd,drive=maindisk,bus=ahci.0
    // }
    let args: Vec<&OsStr> = cmd.get_args().collect();
    println!("running command qemu-system-x86_64 with args");
    args.iter()
        .for_each(|arg| println!("{}", arg.to_str().unwrap()));
    let mut child = cmd.spawn().unwrap();
    // let mut cmd = std::process::Command::new("gdb");
    // let mut gdb_child = cmd.spawn().unwrap();
    // gdb_child.wait().unwrap();
    child.wait().unwrap();
}
