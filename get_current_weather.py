from sys import argv
import json

match argv[1:]:  # ignore "get_current_weather.py"
    case []:
        print(
            json.dumps(
                {
                    "location": "Boston, MA",
                    "temperature": "72",
                    "unit": None,
                    "forecast": ["sunny", "windy"],
                }
            )
        )
    case ["specification"]:
        print(
            json.dumps(
                {
                    "name": "get_current_weather",
                    "description": "Get the current weather in a given location",
                    "parameters": {
                        "type": "object",
                        "required": ["location"],
                        "properties": {
                            "location": {
                                "type": "string",
                                "description": "The city and state, e.g. San Francisco, CA",  # noqa: E501
                            },
                            "unit": {
                                "type": "string",
                                "enum": ["celsius", "fahrenheit"],
                            },
                        },
                    },
                }
            )
        )
