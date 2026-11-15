// Traceability view (tv): six requirements, eight implementers and many-to-many satisfy links.
// One requirement is deliberately unmet; subject membership alone is not a satisfaction claim.
package MaritimeAssurance {
    private import ScalarValues::*;

    part def Equipment {
        attribute enduranceHours : Real;
        attribute coveragePercent : Real;
        attribute reserveMinutes : Real;
    }

    requirement def ContinuousSurvey {
        subject equipment : Equipment;
        require constraint { equipment.enduranceHours >= 12 }
    }
    requirement def Coverage {
        subject equipment : Equipment;
        require constraint { equipment.coveragePercent >= 98 }
    }
    requirement def SafeReturn {
        subject equipment : Equipment;
        require constraint { equipment.reserveMinutes >= 45 }
    }
    requirement def ShoreVisibility {
        doc /* Operators shall receive navigation and payload health reports. */
        subject equipment : Equipment;
    }
    requirement def RecoverableDataset {
        doc /* Survey data shall remain recoverable after a communications outage. */
        subject equipment : Equipment;
    }
    requirement def IceOperations {
        doc /* The future polar variant shall support survey operations in broken ice. */
        subject equipment : Equipment;
    }

    part serviceBattery : Equipment;
    part reserveBattery : Equipment;
    part navigationComputer : Equipment;
    part sonarArray : Equipment;
    part radioModem : Equipment;
    part relayBuoy : Equipment;
    part onboardRecorder : Equipment;
    part shoreArchive : Equipment;

    satisfy ContinuousSurvey by serviceBattery;
    satisfy ContinuousSurvey by sonarArray;
    satisfy Coverage by navigationComputer;
    satisfy Coverage by sonarArray;
    satisfy SafeReturn by reserveBattery;
    satisfy SafeReturn by navigationComputer;
    satisfy ShoreVisibility by radioModem;
    satisfy ShoreVisibility by relayBuoy;
    satisfy ShoreVisibility by navigationComputer;
    satisfy RecoverableDataset by onboardRecorder;
    satisfy RecoverableDataset by shoreArchive;
    satisfy RecoverableDataset by radioModem;
}
