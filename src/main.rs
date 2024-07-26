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
        .arg(format!("format=raw,file={bios_path},if=none,id=bootdisk"));
    cmd.arg("-device")
        .arg("ide-hd,drive=bootdisk,bus=piix4-ide.0");
    cmd.arg("-drive")
        .arg("file=img/disk.img,if=none,format=raw,id=maindisk");
    cmd.arg("-device").arg("ahci,id=ahci");
    cmd.arg("-device").arg("ide-hd,drive=maindisk,bus=ahci.0");
    // -device piix4-ide,bus=pci.0,id=piix4-ide
    // -drive file=${OBJ_FOLDER}/${OS_IMAGE},if=none,format=raw,id=bootdisk\
    //-device ide-hd,drive=bootdisk,bus=piix4-ide.0 \
    // -drive file=img/disk.img,if=none,format=raw,id=maindisk\
    // -device ahci,id=ahci \
    // -device ide-hd,drive=maindisk,bus=ahci.0
    // }
    let mut child = cmd.spawn().unwrap();
    child.wait().unwrap();
}
