// Action flow view (afv): twelve actions with explicit parallel branches, joins and a retry path.
// Successions connect action nodes; no dedicated fork/join, if/loop or swimlane notation is claimed.
package MaritimeActions {
    action def SurveySortie {
        action acceptMission;
        action checkWeather;
        action checkBatteries;
        action calibrateSonar;
        action approveDeparture;
        action leaveHarbour;
        action followTransect;
        action recordSoundings;
        action assessCoverage;
        action returnToHarbour;
        action archiveDataset;
        action publishReport;

        succession acceptMission then checkWeather;
        succession acceptMission then checkBatteries;
        succession acceptMission then calibrateSonar;
        succession checkWeather then approveDeparture;
        succession checkBatteries then approveDeparture;
        succession calibrateSonar then approveDeparture;
        succession approveDeparture then leaveHarbour;
        succession leaveHarbour then followTransect;
        succession followTransect then recordSoundings;
        succession recordSoundings then assessCoverage;
        succession assessCoverage then followTransect;
        succession assessCoverage then returnToHarbour;
        succession returnToHarbour then archiveDataset;
        succession archiveDataset then publishReport;
    }
}
