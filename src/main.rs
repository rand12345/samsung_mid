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
}

async fn keyboard(tx: Sender<Order>) -> Result<(), MyError> {
    let mut reader = tokio::io::BufReader::new(tokio::io::stdin());
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        println!(
            "'r' to read temps, '1' waterloopT up, '2' waterloopT down, 'u' +IndoorTemp, 'd' -IndoorTemp, 'p' +HotWaterSetTemp, 'l' -HotWaterSetTemp, 'c' ch mode, 'w' dhw mode"
        );
        let _fut = reader.read_until(b'\n', &mut buffer).await;
        println!("Input was: {buffer:?}",);
        if let Err(e) = match buffer[0] {
            b'r' => tx.send(Order::Get(Request::Temps)).await,
            b'1' => tx.send(Order::Set(Instruction::WaterOutUp)).await,
            b'2' => tx.send(Order::Set(Instruction::WaterOutDown)).await,
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
    set_flow_temp: i16,
}

impl Pump {
    fn set_mode(&mut self, val: Mode) {
        self.mode = val
    }
    fn flow_up(&mut self) -> Option<i16> {
        // implement bounds checking
        if (0..800i16).contains(&self.flow_temp) {
            self.set_flow_temp = self.target_flow_temp + 5;
            Some(self.set_flow_temp)
        } else {
            None
        }
    }
    fn flow_down(&mut self) -> Option<i16> {
        // implement bounds checking
        if (0..800i16).contains(&self.flow_temp) {
            self.set_flow_temp = self.target_flow_temp - 5;
            Some(self.set_flow_temp)
        } else {
            None
        }
    }
    fn ch_up(&mut self) -> Option<i16> {
        // implement bounds checking
        if (0..800i16).contains(&self.target_indoor_temp) {
            self.set_target_indoor_temp = self.target_indoor_temp + 5;
            Some(self.set_target_indoor_temp)
        } else {
            None
        }
    }
    fn ch_down(&mut self) -> Option<i16> {
        // implement bounds checking
        if (0..800i16).contains(&self.target_indoor_temp) {
            self.set_target_indoor_temp = self.target_indoor_temp - 5;
            Some(self.set_target_indoor_temp)
        } else {
            None
        }
    }
    fn dhw_up(&mut self) -> Option<i16> {
        // implement bounds checking
        if (0..800i16).contains(&self.set_dhw_temp) {
            self.set_dhw_temp = self.dhw_temp + 5;
            Some(self.set_dhw_temp)
        } else {
            None
        }
    }
    fn dhw_down(&mut self) -> Option<i16> {
        // implement bounds checking
        if (0..800i16).contains(&self.set_dhw_temp) {
            self.set_dhw_temp = self.dhw_temp - 5;
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

        /*
            Init unit
            # map Flow rate (l/min), OutdoorT, 3-way valve 0=CH 1=DHW, Compressor controll %, Compressor freq (Hz), Immersion heater status
            src https://github.com/openenergymonitor/emonhub/blob/master/src/interfacers/EmonHubMinimalModbusInterfacer.py
        */

        // println!("Setting extra registers");
        // self.write_multiple(7005, &[0x42E9, 0x8204, 0x4067, 0x42F1, 0x8238, 0x4087])
        //     .await?;
        {
            println!("Reading OffOn state");
            if matches!(self.bus.read_holding_registers(52, 1).await?[0], 0) {
                println!("Pump off, turning on");
                self.bus.write_single_register(52, 1).await?;
                if matches!(self.bus.read_holding_registers(52, 1).await?[0], 1) {
                    println!("Pump On");
                } else {
                    println!("Pump still off")
                }
            } else {
                println!("Pump already on")
            }
            println!("Setting pump mode to heat - testing only");
            self.bus.write_single_register(53, 4).await?;
            println!("Set pump mode to heat - testings only");
        }

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
                            self.read(HotWaterTemp).await?;
                            self.read(HotWaterSetTemp).await?;
                            self.read(WaterOutTemp).await?;
                            self.read(WaterOutSetTemp).await?;
                            self.read(IndoorTemp).await?;
                            self.read(TargetIndoorTemp).await?;
                            self.read(WaterInTemp).await?;
                        }
                    }
                }
                Order::Set(command) => match command {
                    // use set point write val increment (self.val += 1)
                    Instruction::DhwUp => {
                        println!("Command process: {command:?}");
                        if let Some(val) = self.pump.dhw_up() {
                            self.write(WriteReg::HotWaterSetTemp, val as u16).await?;
                            println!(
                                "Sending DhwUP: Reg {:?} {}",
                                WriteReg::HotWaterSetTemp,
                                val as u16
                            )
                        } else {
                            eprintln!("Requested hot water temp (out of range)");
                        };
                    }
                    Instruction::DhwDown => {
                        println!("Command process: {command:?}");
                        if let Some(val) = self.pump.dhw_down() {
                            self.write(WriteReg::HotWaterSetTemp, val as u16).await?;
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
                        self.write(WriteReg::HotWaterOffOn, 1).await?;
                        self.write(WriteReg::OffOn, 0).await?; // not sure if needed
                        self.pump.set_mode(Mode::Dhw);
                    }
                    Instruction::Ch => {
                        println!("Command process: {command:?}");
                        self.write(WriteReg::OffOn, 1).await?;
                        self.write(WriteReg::HotWaterOffOn, 0).await?; // not sure if needed
                        self.pump.set_mode(Mode::Ch);
                    }
                    Instruction::WaterOutUp => {
                        println!("Command process: {command:?}");
                        if let Some(val) = self.pump.flow_up() {
                            self.write(WriteReg::WaterOutSetTemp, val as u16).await?;
                        } else {
                            eprintln!("Requested indoor temp (out of range)");
                        };
                    }
                    Instruction::WaterOutDown => {
                        println!("Command process: {command:?}");
                        if let Some(val) = self.pump.flow_down() {
                            self.write(WriteReg::WaterOutSetTemp, val as u16).await?;
                        } else {
                            eprintln!("Requested indoor temp (out of range)");
                        };
                    }
                },
            }
        }
    }

    async fn readall(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // delay refresh
        sleep(Duration::from_secs(1)).await;
        use ReadReg::*;
        for val in [
            // FlowRate,
            // ThreeWay,
            HotWaterTemp,
            WaterInTemp,
            WaterOutTemp,
            WaterOutSetTemp,
            HotWaterOffOn,
            HotWaterSetTemp,
            OffOn,
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
    async fn write_multiple(
        &mut self,
        reg: u16,
        val: &[u16],
    ) -> Result<(), Box<dyn std::error::Error>> {
        sleep(COMMAND_DELAY).await;
        // elf._rs485.write_registers(7005,[0x42E9, 0x8204, 0x4067, 0x42F1, 0x8238, 0x4087])
        self.bus.write_multiple_registers(reg, val).await?;
        println!("Wrote {val:02x?} to {reg:?}");
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
            // ReadReg::FlowRate => {
            //     self.pump.flow_rate = rsp;
            // }
            ReadReg::HotWaterTemp => {
                self.pump.dhw_temp = rsp as i16;
            }
            ReadReg::WaterInTemp => {
                self.pump.return_temp = rsp as i16;
            }
            ReadReg::WaterOutTemp => {
                self.pump.flow_temp = rsp as i16;
            }
            ReadReg::WaterOutSetTemp => {
                self.pump.target_flow_temp = rsp as i16;
            }
            ReadReg::HotWaterOffOn => {
                // self.pump.dhw_status = rsp == 1;
                self.pump.mode = Mode::Dhw;
            }
            ReadReg::HotWaterSetTemp => {
                self.pump.target_dwh_temp = rsp as i16;
            }
            ReadReg::OffOn => {
                // self.pump.ch_status = rsp == 1;
                self.pump.mode = Mode::Ch;
            }
            ReadReg::IndoorTemp => {
                self.pump.indoor_temp = rsp as i16;
            }
            ReadReg::TargetIndoorTemp => {
                self.pump.target_indoor_temp = rsp as i16;
            }
            _ => unimplemented!(),
        };
    }
}

#[allow(dead_code)]
#[derive(Debug, Copy, Clone)]
enum MessageSetId {}

#[allow(dead_code)]
#[derive(Debug, Copy, Clone)]
enum WriteReg {
    UnitType = 51,
    OffOn = 52,              // 0: Off, 1: On
    AirConditionerMode = 53, //0: Auto, 1: Cool, 2: Dry, 3: Fan, 4: Heat, 21: Cool Storage, 24: Heat Storage
    IndoorTemp = 58,         // Not HW HP!
    WaterOutSetTemp = 68,
    HotWaterOffOn = 72,
    HotWaterMode = 73, // 0: Eco, 1: Standard, 2: Power, 3: Force (for the EHS only)
    HotWaterSetTemp = 74,
    QuietControl = 78, // 0: Normal, 1: SIlent
}

#[allow(dead_code)]
#[derive(Debug, Copy, Clone)]
enum ReadReg {
    CommsStatus = 50,        //b0: Exist, b1: Type OK, b2: Ready, b3: Communication error
    OffOn = 52,              // 0: Off, 1: On
    AirConditionerMode = 53, //0: Auto, 1: Cool, 2: Dry, 3: Fan, 4: Heat, 21: Cool Storage, 24: Heat Storage
    TargetIndoorTemp = 58,
    IndoorTemp = 59,
    ErrorCode = 64, // 0: No Error, 100-999: Error Code
    ThreeWay = 89,
    WaterInTemp = 65,
    WaterOutTemp = 66,
    WaterOutSetTemp = 68,
    HotWaterOffOn = 72,   // 0: Off, 1: On
    HotWaterMode = 73,    // 0: Eco, 1: Standard, 2: Power, 3: Force (for the EHS only)
    HotWaterSetTemp = 74, // Celsius value x10
    HotWaterTemp = 75,    // Celsius value x 10
    QuietControl = 78,    // 0: Normal, 1: Silent
    AwayFunction = 79,
    // FlowRate = 87,
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
    WaterOutUp,
    WaterOutDown,
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
