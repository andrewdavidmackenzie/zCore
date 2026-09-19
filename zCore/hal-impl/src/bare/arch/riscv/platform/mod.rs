mod boot_page_table;
pub(super) mod consts;

cfg_if::cfg_if! {
    if #[cfg(any(feature = "board-c910light", feature = "board-fu740"))] {
        mod entry64;
    } else {
        mod entry;
    }
}
