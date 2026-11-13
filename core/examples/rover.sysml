package RoverSystem {
    private import ScalarValues::*;

    attribute def MassValue;

    part def Wheel {
        attribute radius : Real;
        attribute load : Real;
    }

    part def Chassis {
        part front : Wheel;
        part rear : Wheel;
        attribute mass : MassValue;
    }

    part def Rover {
        part chassis : Chassis;
        part left : Wheel;
        attribute mass : MassValue;
        attribute speed : Real;
    }

    requirement def SlopeLimit {
        doc /* The rover shall climb a 25 degree slope fully loaded. */
        subject r : Rover;
        require constraint { r.mass <= 40 [kg] }
    }

    requirement def RangeLimit {
        subject r : Rover;
        require constraint { r.speed >= 0.5 [m/s] }
    }

    part rover : Rover;
    satisfy SlopeLimit by rover;
    satisfy RangeLimit by rover;
}

package RoverOps {
    part def Base {
        part dock : RoverSystem::Rover;
    }

    part base : Base;
}
