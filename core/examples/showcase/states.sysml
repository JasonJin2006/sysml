// State transition view (stv): an eight-state mission controller and a four-state radio controller.
// Guards and cross-links are explicit; nested-machine entry/do/exit compartments are not implied.
package MaritimeStates {
    private import ScalarValues::*;

    state def MissionController {
        attribute batteryLevel : Real;
        attribute surveyComplete : Boolean;
        attribute linkHealthy : Boolean;
        state berthed;
        state checking;
        state ready;
        state underway;
        state surveying;
        state returning;
        state docking;
        state emergency;

        transition beginChecks first berthed then checking;
        transition checksPassed first checking if batteryLevel >= 80 then ready;
        transition depart first ready then underway;
        transition arriveAtTransect first underway then surveying;
        transition finishSurvey first surveying if surveyComplete then returning;
        transition lowBattery first surveying if batteryLevel < 25 then returning;
        transition returnToHarbour first returning then docking;
        transition docked first docking then berthed;
        transition abortDeparture first underway if batteryLevel < 20 then returning;
        transition lossOfLink first surveying if not linkHealthy then emergency;
        transition recoverLink first emergency if linkHealthy then returning;
        transition restartChecks first emergency then checking;
    }

    state def RadioController {
        state disconnected;
        state acquiring;
        state linked;
        state degraded;
        transition search first disconnected then acquiring;
        transition established first acquiring then linked;
        transition interference first linked then degraded;
        transition recovered first degraded then linked;
        transition timedOut first degraded then disconnected;
    }
}
