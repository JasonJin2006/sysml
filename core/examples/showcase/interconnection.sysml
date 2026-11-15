// Interconnection view (iv): three-level equipment containment, conjugated ports and nine wires.
// Binary port connections are shown; this does not claim dedicated interface or flow notation.
package MaritimeInterconnection {
    private import ScalarValues::*;

    port def DataLink { out packet : Real; }
    port def PowerFeed { out watts : Real; }

    part def NavigationComputer {
        out port portCommand : DataLink;
        out port starboardCommand : DataLink;
        out port sonarCommand : DataLink;
        in port telemetryIn : ~DataLink;
        in port powerIn : ~PowerFeed;
    }
    part def MotorDrive {
        in port commandIn : ~DataLink;
        in port powerIn : ~PowerFeed;
    }
    part def SurveySonar {
        in port commandIn : ~DataLink;
        out port soundingsOut : DataLink;
        in port powerIn : ~PowerFeed;
    }
    part def RadioModem {
        out port telemetryOut : DataLink;
        in port powerIn : ~PowerFeed;
    }
    part def DistributionUnit {
        out port computerFeed : PowerFeed;
        out port motorFeed : PowerFeed;
        out port sonarFeed : PowerFeed;
        out port radioFeed : PowerFeed;
    }
    part def ControlBay {
        part navigator : NavigationComputer;
        part radio : RadioModem;
    }
    part def PayloadBay { part sonar : SurveySonar; }
    part def PropulsionBay {
        part portDrive : MotorDrive;
        part starboardDrive : MotorDrive;
    }
    part def SurveyCraft {
        part control : ControlBay;
        part payloadBay : PayloadBay;
        part propulsion : PropulsionBay;
        part powerUnit : DistributionUnit;

        connect control.navigator.portCommand to propulsion.portDrive.commandIn;
        connect control.navigator.starboardCommand to propulsion.starboardDrive.commandIn;
        connect control.navigator.sonarCommand to payloadBay.sonar.commandIn;
        connect payloadBay.sonar.soundingsOut to control.navigator.telemetryIn;
        connect control.radio.telemetryOut to control.navigator.telemetryIn;
        connect powerUnit.computerFeed to control.navigator.powerIn;
        connect powerUnit.motorFeed to propulsion.portDrive.powerIn;
        connect powerUnit.sonarFeed to payloadBay.sonar.powerIn;
        connect powerUnit.radioFeed to control.radio.powerIn;
    }

    part pelican : SurveyCraft;
}
