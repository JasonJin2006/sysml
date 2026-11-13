package SurveyDrone {
    private import ISQ::*;
    private import SI::*;
    private import ScalarValues::*;

    part def Battery {
        attribute energy : EnergyValue = 360 [W*h];
        attribute mass : MassValue = 2.1 [kg];
    }

    part def Airframe {
        attribute mass : MassValue = 3.4 [kg];
    }

    part def Drone {
        part battery : Battery;
        part airframe : Airframe;
        attribute hoverPower : PowerValue = 900 [W];
        attribute takeoffMass : MassValue = battery.mass + airframe.mass;
        attribute endurance : DurationValue = battery.energy / hoverPower;
    }

    requirement def HoverEndurance {
        doc /* The drone shall hover for at least 20 minutes on one battery. */
        subject d : Drone;
        require constraint { d.endurance >= 20 [min] }
    }

    requirement def TakeoffMassLimit {
        doc /* The drone shall weigh no more than 6 kg at takeoff. */
        subject d : Drone;
        require constraint { d.takeoffMass <= 6 [kg] }
    }

    part drone : Drone;
    satisfy HoverEndurance by drone;
    satisfy TakeoffMassLimit by drone;
}
