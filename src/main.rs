#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use tokio_serial::SerialStream;

    use tokio_modbus::prelude::*;

    let tty_path = "/dev/ttyUSB0";
    let slave = Slave(0x1);

    let builder = tokio_serial::new(tty_path, 9600);
    let port = SerialStream::open(&builder).unwrap();
    /*
    instrument.write_registers(7005,[0x42E9, 0x42F1, 0x4067, 0x8204])
     */
    let mut ctx = rtu::connect_slave(port, slave).await?;
    println!("Reading a sensor value");
    // let _a = ctx
    //     .write_multiple_registers(7005, &[0x42E9, 0x42F1, 0x4067, 0x8204])
    //     .await?;
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
}
