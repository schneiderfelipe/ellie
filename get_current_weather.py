from sys import argv
from subprocess import call

match argv[1:]:  # ignore "get_current_weather.py"
    case ["specification"]:
        call(["cat", "current_weather_specification.json"])
    case []:
        call(["cat", "current_weather.json"])
