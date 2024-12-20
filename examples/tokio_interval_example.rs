use std::time::Duration;

use tokio::{
    select,
    sync::mpsc::unbounded_channel,
    time::{self, interval, sleep, timeout},
};

#[tokio::main]
async fn main() {
    let h = tokio::spawn(timeout(Duration::from_millis(500), async move {
        println!("666");
        time::sleep(Duration::from_secs(1)).await;
        println!("12312312")
    }));
    h.await;
}
