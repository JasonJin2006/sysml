// Package view (pv): four namespace levels and three independent engineering branches.
// Frames show containment; imports are intentionally not presented as dependency arrows.
package MaritimeProgramme {
    part def Fleet;
    part demonstrationFleet : Fleet;

    package Platform {
        part def Hull;
        part def Deck;
        part hull : Hull;
        package Propulsion {
            part def Motor;
            part def Propeller;
            part motor : Motor;
            package Bearings {
                part def ShaftBearing;
                part def Seal;
                part forwardBearing : ShaftBearing;
                part aftSeal : Seal;
            }
        }
    }

    package Survey {
        part def SurveyPackage;
        part hydrography : SurveyPackage;
        package Acoustic {
            part def SonarHead;
            part def EchoProcessor;
            part sonar : SonarHead;
        }
        package Environmental {
            part def WaterProbe;
            part probe : WaterProbe;
        }
    }

    package Operations {
        part def ShoreStation;
        part harbourStation : ShoreStation;
        package Logistics {
            part def TransportCradle;
            part transportCradle : TransportCradle;
        }
        package Communications {
            part def RelayBuoy;
            part northBuoy : RelayBuoy;
        }
    }
}
