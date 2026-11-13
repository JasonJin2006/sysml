package TypoProbe {
    private import ScalarValues::*;
    part def Drone { attribute endurance : Real; }
    requirement def R {
        subject d : Drone;
        require constraint { d.endurancee >= 20 }
    }
}