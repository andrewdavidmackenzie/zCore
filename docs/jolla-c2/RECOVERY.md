# Recovery Procedures for Jolla C2

## Restoring stock bootloader (if U-Boot causes problems)

The backup of the original `boot_a` partition was saved to the SD card
as `boot_a_backup.bin` during the initial U-Boot flashing process.

### Method 1: From Sailfish OS recovery mode

1. Power off the phone
2. Remove the SD card
3. Hold **Vol Up + Power** — select recovery mode with **Vol Down**
4. In recovery mode, mount the SD card:
   ```
   mount -t ext4 /dev/mmcblk1p1 /mnt
   ```
5. Restore the original bootloader:
   ```
   dd if=/mnt/boot_a_backup.bin of=/dev/disk/by-partlabel/boot_a bs=4096
   dd if=/mnt/boot_a_backup.bin of=/dev/disk/by-partlabel/boot_b bs=4096
   ```
6. Reboot:
   ```
   reboot
   ```

### Method 2: From stock Sailfish OS

1. Remove the SD card
2. Power on — if U-Boot can't find the SD card, it may fall back to
   booting stock Sailfish from eMMC, or it may hang at a U-Boot prompt
3. If stock Sailfish boots:
   - Insert SD card
   - Open Terminal, run `devel-su`
   - Mount SD card: `mount -t ext4 /dev/mmcblk1p1 /mnt`
   - Restore: `dd if=/mnt/boot_a_backup.bin of=/dev/disk/by-partlabel/boot_a bs=4096`
   - Also: `dd if=/mnt/boot_a_backup.bin of=/dev/disk/by-partlabel/boot_b bs=4096`
   - Reboot

### Method 3: Via spd_dump (if phone won't boot at all)

If the phone is completely unbootable (no recovery, no Sailfish):

1. Get `fdl2-dl.bin` from Jolla/Reeder stock firmware
2. Put the phone in Unisoc download mode (power off, hold Vol Up, plug USB)
3. Use spd_dump to restore:
   ```bash
   cd prebuilt
   ../spreadtrum_flash/spd_dump --wait 60 \
     exec_addr 0x65015f08 \
     fdl fdl1-sign.bin 0x65000800 \
     fdl <fdl2-dl.bin> 0x9efffe00 \
     exec \
     write_part boot_a boot_a_backup.bin \
     write_part boot_b boot_a_backup.bin \
     power_off
   ```

## Breaking the reboot loop

If the mainline kernel causes an infinite reboot loop:

1. **Remove the SD card** — pull the SIM/SD tray while the phone is rebooting.
   Without the SD card, U-Boot has no kernel to load and should stop or
   fall back to eMMC.

2. If that doesn't work, **hold power for 15+ seconds** to force power off,
   then remove the SD card before powering on.

3. U-Boot without an SD card should either:
   - Show a U-Boot shell prompt (if configured to stop on boot failure)
   - Fall back to booting from eMMC (stock Sailfish)
   - Hang (power off with long press)

## Important files on the SD card

| File | Purpose |
|------|---------|
| `boot_a_backup.bin` | Original boot_a partition — DO NOT DELETE |
| `u-boot-2nd.img` | U-Boot image that was flashed |
| `Image` | Linux mainline kernel |
| `extlinux/extlinux.conf` | Boot configuration |
| `ums9230-reeder-s19mps.dtb` | Device tree |
