use tokio::time::{sleep, Duration};
use tokio_modbus::{client::Context, prelude::Reader};

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
    let ctx = rtu::connect_slave(port, slave).await?;
    let mut device = Device {
        pump: Pump::default(),
        bus: ctx,
    };
    use ReadReg::*;
    for val in [
        FlowRate,
        // ThreeWay,
        DhwTemp,
        ReturnTemp,
        FlowTemp,
        TargetFlowTemp,
        DhwStatus,
        TargetDwhTemp,
        ChStatus,
        IndoorTemp,
        TargetIndoorTemp,
    ] {
        device.read(val).await?;
        delay_ms(100).await;
    }

    Ok(())
}

#[allow(dead_code)]
struct Device {
    pump: Pump,
    bus: Context,
}

impl Device {
    async fn read(&mut self, val: ReadReg) -> Result<(), Box<dyn std::error::Error>> {
        println!("Reading a sensor value {val:?}");
        let rsp = self.bus.read_holding_registers(val as u16, 1).await?;
        println!("Sensor value is: {rsp:?} for {:?}", ReadReg::FlowRate);
        Ok(())
    }
}

#[allow(dead_code)]
#[derive(Debug, Default)]
struct Pump {
    flow_rate: u16,
    // three_way: bool,
    dhw_temp: i16,
    return_temp: i16,
    flow_temp: i16,
    target_flow_temp: i16,
    dhw_status: bool,
    target_dwh_temp: i16,
    ch_status: bool,
    indoor_temp: i16,
    target_indoor_temp: i16,
}

impl Pump {}

#[derive(Debug)]
enum ReadReg {
    FlowRate = 87,
    // ThreeWay = 89,
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
