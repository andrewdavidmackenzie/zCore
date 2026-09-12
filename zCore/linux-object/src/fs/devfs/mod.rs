mod fbdev;
// input devfs module removed: input drivers moved out of kernel (#237)
// mod input;
mod random;
mod uartdev;

pub use fbdev::FbDev;
// pub use input::{EventDev, MiceDev};  // removed: input drivers moved out (#237)
pub use random::RandomINode;
pub use uartdev::UartDev;
