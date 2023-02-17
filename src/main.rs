use tokio::time::{sleep, Duration};

async fn delay_ms(dur: u64) {
    sleep(Duration::from_millis(dur)).await;
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use tokio_serial::SerialStream;

    use tokio_modbus::prelude::*;

    let tty_path = "/dev/ttyUSB0";
    let slave = Slave(0x1);

    let builder = tokio_serial::new(tty_path, 9600).parity(tokio_serial::Parity::Even);
    let port = SerialStream::open(&builder).unwrap();
    /*
    instrument.write_registers(7005,[0x42E9, 0x42F1, 0x4067, 0x8204])
    */
    let mut ctx = rtu::connect_slave(port, slave).await?;
    delay_ms(500).await;
    println!("Sending init - enabling regs");
    ctx.write_multiple_registers(7005, &[0x42E9, 0x42F1, 0x4067, 0x8204])
        .await?;
    println!("Reading a sensor value");
    delay_ms(500).await;
    let rsp = ctx
        .read_holding_registers(ReadReg::FlowRate as u16, 2)
        .await?;
    println!("Sensor value is: {rsp:?} for {:?}", ReadReg::FlowRate);

    Ok(())
}

#[derive(Debug)]
enum ReadReg {
    FlowRate = 87,
    ThreeWay = 89,
    DhwTemp = 75,
    ReturnTemp = 65,
    FlowTemp = 66,
    TargetFlowTemp = 68,
    DhwStatus = 72,
    TargetDwhTemp = 74,
    ChStatus = 52,
    IndoorTemp = 59,
    TargetIndoorTemp = 58,
}
