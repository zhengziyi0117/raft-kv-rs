
use tokio::signal::ctrl_c;

#[tokio::main]
async fn main() {
    let handle1 = tokio::spawn(async move {
        ctrl_c().await;
        println!("123");
    });
    let handle2 = tokio::spawn(async move {
        ctrl_c().await;
        println!("456");
    });
    let handle3 = tokio::spawn(async move {
        ctrl_c().await;
        println!("789");
    });
    let vs = vec![handle1, handle2, handle3];
    for v in vs {
        v.await;
    }
}
