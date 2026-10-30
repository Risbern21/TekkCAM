use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortInfo {
    pub name: String,
    pub description: String,
}

pub fn available_ports() -> Result<Vec<PortInfo>, String> {
    serialport::available_ports()
        .map(|ports| {
            ports
                .into_iter()
                .map(|port| {
                    let description = match port.port_type {
                        serialport::SerialPortType::UsbPort(info) => {
                            let manufacturer = info.manufacturer.unwrap_or_default();
                            let product = info.product.unwrap_or_default();
                            let label = format!("{manufacturer} {product}").trim().to_owned();
                            if label.is_empty() {
                                "USB serial".into()
                            } else {
                                label
                            }
                        }
                        serialport::SerialPortType::BluetoothPort => "Bluetooth serial".into(),
                        serialport::SerialPortType::PciPort => "PCI serial".into(),
                        serialport::SerialPortType::Unknown => "Serial port".into(),
                    };
                    PortInfo {
                        name: port.port_name,
                        description,
                    }
                })
                .collect()
        })
        .map_err(|error| error.to_string())
}

pub fn open_port(name: &str, baud_rate: u32) -> Result<Box<dyn serialport::SerialPort>, String> {
    serialport::new(name, baud_rate)
        .timeout(Duration::from_millis(20))
        .open()
        .map_err(|error| error.to_string())
}
