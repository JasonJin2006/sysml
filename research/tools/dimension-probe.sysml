package DimensionProbe {
    private import ScalarValues::*;
    private import ISQ::*;
    private import SI::*;

    part def Vehicle {
        attribute mass : MassValue;
        attribute topSpeed : SpeedValue;
        attribute endurance : TimeValue;
    }

    part vehicle : Vehicle {
        attribute mass = 1200 [kg];
        attribute topSpeed = 55 [m/s];
        // 量纲错误：时间接受了一个长度值
        attribute endurance = 40 [m];
    }

    // 量纲错误：拿时间跟长度比较
    requirement def EnduranceCheck {
        subject v : Vehicle;
        require constraint { v.endurance >= 20 [min] }
    }

    // 量纲错误：把质量除以速度，当成时间
    attribute wrongCalc : TimeValue = 100 [kg] / 10 [m/s];
}
