// General view (gv): shared types, specialization, variation alternatives and requirement traces.
// Uses the current general-view notation; it is not a catalogue of every SysML graphical symbol.
package MaritimeArchitecture {
    private import ScalarValues::*;

    part def Sensor {
        attribute sampleRate : Real;
        attribute mass : Real;
    }
    part def Battery {
        attribute energy : Real;
        attribute mass : Real;
    }
    part def Thruster {
        attribute peakPower : Real;
        attribute thrust : Real;
    }
    part def SurveyVessel {
        attribute dryMass : Real;
        attribute enduranceHours : Real;
        part forwardSonar : Sensor;
        part aftSonar : Sensor;
        part weatherStation : Sensor;
        part portThruster : Thruster;
        part starboardThruster : Thruster;
        part serviceBattery : Battery;
        part emergencyBattery : Battery;
    }
    part def LongRangeBattery :> Battery {
        attribute cycleLife : Real;
    }
    variation part def EnergyChoices :> Battery {
        variant part standardPack : Battery;
        variant part oceanPack : LongRangeBattery;
    }

    requirement def SurveyEndurance {
        doc /* The vessel shall survey continuously for at least twelve hours. */
        subject vessel : SurveyVessel;
        require constraint { vessel.enduranceHours >= 12 }
    }
    requirement def TransportMass {
        doc /* The unloaded vessel shall fit the road transport mass budget. */
        subject vessel : SurveyVessel;
        require constraint { vessel.dryMass <= 1800 }
    }
    requirement def MissionAvailability {
        doc /* Navigation and survey payloads shall remain available after a single battery outage. */
        subject vessel : SurveyVessel;
    }

    part pelican : SurveyVessel;
    satisfy SurveyEndurance by pelican;
    satisfy TransportMass by pelican;
    satisfy MissionAvailability by pelican;
}
