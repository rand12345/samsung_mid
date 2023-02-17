use tokio::io::AsyncBufReadExt;
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

    let builder = tokio_serial::new(tty_path, 9600)
        .parity(tokio_serial::Parity::Even)
        .timeout(Duration::from_millis(100));
    let port = SerialStream::open(&builder).unwrap();
    let ctx = rtu::connect_slave(port, slave).await?;

    let mut device = Device {
        pump: Pump::default(),
        bus: ctx,
    };
    device.looper().await?;

    Ok(())
}

async fn do_some_work(task_name: &str) -> Result<(), MyError> {
    println!("This is task {}, doing some work", task_name);
    let mut reader = tokio::io::BufReader::new(tokio::io::stdin());
    let mut buffer = Vec::new();

    let fut = reader.read_until(b'\n', &mut buffer).await;
    println!("Input was: {:?}", buffer);
    Ok(())
}

#[allow(dead_code)]
struct Device {
    pump: Pump,
    bus: Context,
}

impl Device {
    async fn looper(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        loop {
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
                self.read(val).await?;
            }
            delay_ms(2000).await;
        }
    }
    async fn read(&mut self, val: ReadReg) -> Result<(), Box<dyn std::error::Error>> {
        delay_ms(10).await;
        print!("Reading a sensor value {val:?}... ");
        let rsp = self.bus.read_holding_registers(val as u16, 1).await?;
        println!("Sensor value is: {rsp:?} for {val:?}");
        self.decode(rsp[0], val);
        Ok(())
    }
    fn decode(&mut self, rsp: u16, val: ReadReg) {
        match val {
            ReadReg::FlowRate => {
                self.pump.flow_rate = rsp;
            }
            ReadReg::DhwTemp => {
                self.pump.dhw_temp = rsp as i16;
            }
            ReadReg::ReturnTemp => {
                self.pump.return_temp = rsp as i16;
            }
            ReadReg::FlowTemp => {
                self.pump.flow_temp = rsp as i16;
            }
            ReadReg::TargetFlowTemp => {
                self.pump.target_flow_temp = rsp as i16;
            }
            ReadReg::DhwStatus => {
                self.pump.dhw_status = rsp == 1;
            }
            ReadReg::TargetDwhTemp => {
                self.pump.target_dwh_temp = rsp as i16;
            }
            ReadReg::ChStatus => {
                self.pump.ch_status = rsp == 1;
            }
            ReadReg::IndoorTemp => {
                self.pump.indoor_temp = rsp as i16;
            }
            ReadReg::TargetIndoorTemp => {
                self.pump.target_indoor_temp = rsp as i16;
            }
        };
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

#[derive(Debug, Copy, Clone)]
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

#[derive(Debug)]
enum MyError {
    Other,
}

impl std::error::Error for MyError {}

impl std::fmt::Display for MyError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Other => write!(f, "Some other error occured!"),
        }
    }
}
