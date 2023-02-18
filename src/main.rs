use tokio::io::AsyncBufReadExt;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::{sleep, Duration};
use tokio_modbus::prelude::Writer;
use tokio_modbus::{client::Context, prelude::Reader};

const COMMAND_DELAY: Duration = Duration::from_millis(100);

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use tokio_serial::SerialStream;

    use tokio_modbus::prelude::*;

    let tty_path = "/dev/ttyUSB0";
    let slave = Slave(0x1);

    let builder = tokio_serial::new(tty_path, 9600)
        .parity(tokio_serial::Parity::Even)
        .timeout(COMMAND_DELAY);
    let port = match SerialStream::open(&builder) {
        Ok(s) => s,
        Err(e) => panic!("Serial port must be {tty_path} :: {e}"),
    };
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
    loop {
        sleep(COMMAND_DELAY).await;
    }
    Ok(())
}

async fn keyboard(tx: Sender<Order>) -> Result<(), MyError> {
    let mut reader = tokio::io::BufReader::new(tokio::io::stdin());
    let mut buffer = Vec::new();
    loop {
        println!(
            "'r' to read temps, 'u' +ch, 'd' -ch, 'p' +dwh, 'l' -dhw, 'c' ch mode, 'w' dhw mode"
        );
        let _fut = reader.read_until(b'\n', &mut buffer).await;
        println!("Input was: {buffer:?}",);
        if let Err(e) = match buffer[0] {
            b'r' => tx.send(Order::Get(Request::Temps)).await,
            b'u' => tx.send(Order::Set(Instruction::ChUp)).await,
            b'd' => tx.send(Order::Set(Instruction::ChDown)).await,
            b'p' => tx.send(Order::Set(Instruction::DhwUp)).await,
            b'l' => tx.send(Order::Set(Instruction::DhwDown)).await,
            b'c' => tx.send(Order::Set(Instruction::Ch)).await,
            b'w' => tx.send(Order::Set(Instruction::Dwh)).await,
            _ => {
                buffer.clear();
                continue;
            }
        } {
            eprintln!("MPSC error {e:?}")
        };
    }
}

#[derive(Debug, Default)]
enum Mode {
    Dhw,
    #[default]
    Ch,
}

#[allow(dead_code)]
#[derive(Debug, Default)]
struct Pump {
    mode: Mode,
    flow_rate: u16,
    // three_way: bool,
    dhw_temp: i16,
    return_temp: i16,
    flow_temp: i16,
    target_flow_temp: i16,
    // dhw_status: bool,
    target_dwh_temp: i16,
    // ch_status: bool,
    indoor_temp: i16,
    target_indoor_temp: i16,
    set_target_indoor_temp: i16,
    set_dhw_temp: i16,
}

impl Pump {
    fn set_mode(&mut self, val: Mode) {
        self.mode = val
    }
    fn ch_up(&mut self) -> Option<i16> {
        // implement bounds checking
        if (0..80i16).contains(&self.target_indoor_temp) {
            self.set_target_indoor_temp = self.target_indoor_temp + 1;
            Some(self.set_target_indoor_temp)
        } else {
            None
        }
    }
    fn ch_down(&mut self) -> Option<i16> {
        // implement bounds checking
        if (0..80i16).contains(&self.target_indoor_temp) {
            self.set_target_indoor_temp = self.target_indoor_temp - 1;
            Some(self.set_target_indoor_temp)
        } else {
            None
        }
    }
    fn dhw_up(&mut self) -> Option<i16> {
        // implement bounds checking
        if (0..80i16).contains(&self.set_dhw_temp) {
            self.set_dhw_temp = self.dhw_temp + 1;
            Some(self.set_dhw_temp)
        } else {
            None
        }
    }
    fn dhw_down(&mut self) -> Option<i16> {
        // implement bounds checking
        if (0..80i16).contains(&self.set_dhw_temp) {
            self.set_dhw_temp = self.dhw_temp - 1;
            Some(self.set_dhw_temp)
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
            self.readall().await?;
            println!("Read vals");

            let recv = match rx.recv().await {
                Some(v) => v,
                None => continue,
            };

            match recv {
                Order::Get(request) => {
                    println!("Received request {request:?}");
                    match request {
                        Request::Temps => {
                            self.read(DhwTemp).await?;
                            self.read(TargetDwhTemp).await?;
                            self.read(FlowTemp).await?;
                            self.read(TargetFlowTemp).await?;
                            self.read(IndoorTemp).await?;
                            self.read(TargetIndoorTemp).await?;
                            self.read(ReturnTemp).await?;
                        }
                    }
                }
                Order::Set(command) => match command {
                    // use set point write val increment (self.val += 1)
                    Instruction::DhwUp => {
                        println!("Command process: {command:?}");
                        if let Some(val) = self.pump.dhw_up() {
                            self.write(WriteReg::DhwTemp, val as u16).await?;
                        } else {
                            eprintln!("Requested hot water temp (out of range)");
                        };
                    }
                    Instruction::DhwDown => {
                        println!("Command process: {command:?}");
                        if let Some(val) = self.pump.dhw_down() {
                            self.write(WriteReg::DhwTemp, val as u16).await?;
                        } else {
                            eprintln!("Requested hot water temp (out of range)");
                        };
                    }
                    Instruction::ChUp => {
                        println!("Command process: {command:?}");
                        if let Some(val) = self.pump.ch_up() {
                            self.write(WriteReg::IndoorTemp, val as u16).await?;
                        } else {
                            eprintln!("Requested indoor temp (out of range)");
                        };
                    }
                    Instruction::ChDown => {
                        println!("Command process: {command:?}");
                        if let Some(val) = self.pump.ch_down() {
                            self.write(WriteReg::IndoorTemp, val as u16).await?;
                        } else {
                            eprintln!("Requested indoor temp (out of range)");
                        };
                    }
                    Instruction::Dwh => {
                        println!("Command process: {command:?}");
                        self.write(WriteReg::DhwMode, 1).await?;
                        self.write(WriteReg::ChMode, 0).await?; // not sure if needed
                        self.pump.set_mode(Mode::Dhw);
                    }
                    Instruction::Ch => {
                        println!("Command process: {command:?}");
                        self.write(WriteReg::ChMode, 1).await?;
                        self.write(WriteReg::DhwMode, 0).await?; // not sure if needed
                        self.pump.set_mode(Mode::Ch);
                    }
                },
            }
        }
    }

    async fn readall(&mut self) -> Result<(), Box<dyn std::error::Error>> {
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
        Ok(())
    }

    async fn write(&mut self, reg: WriteReg, val: u16) -> Result<(), Box<dyn std::error::Error>> {
        sleep(COMMAND_DELAY).await;
        self.bus.write_single_register(reg as u16, val).await?;
        println!("Wrote {val} to {reg:?}");
        Ok(())
    }
    async fn read(&mut self, val: ReadReg) -> Result<(), Box<dyn std::error::Error>> {
        sleep(COMMAND_DELAY).await;
        print!("Reading a sensor value {val:?}: ");
        let rsp = self.bus.read_holding_registers(val as u16, 1).await?;
        println!("{rsp:?}");
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
                // self.pump.dhw_status = rsp == 1;
                self.pump.mode = Mode::Dhw;
            }
            ReadReg::TargetDwhTemp => {
                self.pump.target_dwh_temp = rsp as i16;
            }
            ReadReg::ChStatus => {
                // self.pump.ch_status = rsp == 1;
                self.pump.mode = Mode::Ch;
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
    Dwh,
    Ch,
}

#[allow(dead_code)]
#[derive(Debug)]
enum Request {
    Temps,
}

#[derive(Debug)]
enum Order {
    Get(Request),
    Set(Instruction),
}
