use tokio::io::AsyncBufReadExt;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::{sleep, Duration};
use tokio_modbus::prelude::Writer;
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

    let (tx, rx) = mpsc::channel(10);

    tokio::spawn(async move {
        keyboard(tx).await.unwrap();
    });

    tokio::spawn(async move {
        device.looper(rx).await.unwrap();
    });

    Ok(())
}

async fn keyboard(tx: Sender<Order>) -> Result<(), MyError> {
    let mut reader = tokio::io::BufReader::new(tokio::io::stdin());
    let mut buffer = Vec::new();
    loop {
        println!("r to read, u +ch, d -ch");
        let _fut = reader.read_until(b'\n', &mut buffer).await;
        println!("Input was: {buffer:?}",);
        if let Err(e) = match buffer[0] {
            b'r' => tx.send(Order::Get(Request::DhwTemp)).await,
            b'u' => tx.send(Order::Set(Instruction::ChUp)).await,
            b'd' => tx.send(Order::Set(Instruction::ChDown)).await,
            _ => {
                buffer.clear();
                continue;
            }
        } {
            eprintln!("MPSC error {e:?}")
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
    set_target_indoor_temp: i16,
}

impl Pump {
    fn ch_up(&mut self) -> Option<i16> {
        // implement bounds checking
        if (0..40i16).contains(&self.target_indoor_temp) {
            self.set_target_indoor_temp = self.target_indoor_temp + 1;
            Some(self.set_target_indoor_temp)
        } else {
            None
        }
    }
    fn ch_down(&mut self) -> Option<i16> {
        // implement bounds checking
        if (0..40i16).contains(&self.target_indoor_temp) {
            self.set_target_indoor_temp = self.target_indoor_temp - 1;
            Some(self.set_target_indoor_temp)
        } else {
            None
        }
    }
}

#[allow(dead_code)]
struct Device {
    pump: Pump,
    bus: Context,
}

impl Device {
    async fn looper(&mut self, mut rx: Receiver<Order>) -> Result<(), Box<dyn std::error::Error>> {
        use ReadReg::*;
        loop {
            self.readall().await;
            println!("Read vals");

            match rx.try_recv().unwrap() {
                // poll this
                Order::Get(request) => {
                    println!("Received request {request:?}");
                    match request {
                        Request::DhwTemp => self.read(DhwTemp).await?,
                        Request::ChTemp => self.read(FlowTemp).await?,
                    }
                }
                Order::Set(command) => match command {
                    // use set point write val increment (self.val += 1)
                    Instruction::DhwUp => println!("Command process: {command:?}"),
                    Instruction::DhwDown => println!("Command process: {command:?}"),
                    Instruction::ChUp => {
                        println!("Command process: {command:?}");
                        if let Some(val) = self.pump.ch_up() {
                            self.write(WriteReg::IndoorTemp, val as u16).await;
                        } else {
                            eprintln!("Requested indoor temp (out of range)");
                        };
                    }
                    Instruction::ChDown => {
                        println!("Command process: {command:?}");
                        if let Some(val) = self.pump.ch_down() {
                            self.write(WriteReg::IndoorTemp, val as u16).await;
                        } else {
                            eprintln!("Requested indoor temp (out of range)");
                        };
                    }
                    Instruction::Dwh(_) => println!("Command process: {command:?}"),
                    Instruction::Ch(_) => println!("Command process: {command:?}"),
                },
            }
        }
    }

    async fn readall(&mut self) {
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
            self.read(val).await;
        }
    }

    async fn write(&mut self, reg: WriteReg, val: u16) -> Result<(), Box<dyn std::error::Error>> {
        delay_ms(100).await;
        self.bus.write_single_register(reg as u16, val).await?;
        println!("Wrote {val} to {reg:?}");
        Ok(())
    }
    async fn read(&mut self, val: ReadReg) -> Result<(), Box<dyn std::error::Error>> {
        delay_ms(100).await;
        print!("Reading a sensor value {val:?}... ");
        let rsp = self.bus.read_holding_registers(val as u16, 1).await?;
        // println!("Sensor value is: {rsp:?} for {val:?}");
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
#[derive(Debug, Copy, Clone)]
enum WriteReg {
    IndoorTemp = 58,
    DhwTemp = 74,
    ChMode = 52,
    DhwMode = 72,
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
#[derive(Debug)]
enum Instruction {
    DhwUp,
    DhwDown,
    ChUp,
    ChDown,
    Dwh(bool),
    Ch(bool),
}

#[allow(dead_code)]
#[derive(Debug)]
enum Request {
    DhwTemp,
    ChTemp,
}

#[derive(Debug)]
enum Order {
    Get(Request),
    Set(Instruction),
}
