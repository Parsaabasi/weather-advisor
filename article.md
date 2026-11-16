# A little screen by the door that tells you what to wear

We turned an ESP32-S31-Korvo-1 into a weather advisor for Brno. You switch it on, it joins the Wi-Fi, looks up the weather, and writes a short note on its screen: how cold it is, whether you need a coat, whether to take an umbrella, and how strong the sun is. Then it keeps that note fresh on its own.

This is the story of how we got there, told without any code.

## What we set out to make

The idea was simple. Before you leave the house, the board should answer three questions:

- What should I wear for this temperature?
- Do I need an umbrella, waterproof shoes, or a warning about ice?
- Is the sun strong enough that I should bother with sunscreen?

We picked Brno, in Czechia, as the city. We picked a free public weather service called Open-Meteo, so there is no account and no subscription. The board already has Wi-Fi and a color screen, so we did not add a thermometer or any other sensor. The “weather station” is really a small computer that asks the internet, then explains the answer in plain words.

## The board, in everyday terms

The ESP32-S31-Korvo-1 is a development board: a tiny computer on a circuit board, made for trying ideas. Ours is paired with a 4.3 inch color screen, 800 dots wide and 480 dots tall, the kind of panel you might see on a small appliance.

Two things mattered for this project:

- It can join a normal home or office Wi-Fi network.
- The screen is already wired the way Espressif’s reference demos expect. We did not change any cables.

It stays powered all the time. There is no button to press for an update. It looks after itself.

## Step 1. We wrote the advice in sentences first

Before anything appeared on the screen, we agreed the exact words the board would use. Temperature decides the outfit:

- Below 0 °C: a heavy winter coat, warm layers, gloves, and a hat.
- 0 to 9 °C: a warm coat.
- 10 to 15 °C: a lighter coat or a jacket.
- 16 to 21 °C: a light jacket or a cardigan.
- Above 21 °C: light, breathable clothes.

The original plan also said “hot weather” should mention water and sun protection, without naming a number. We chose 25 °C as that line. At 25 °C and above, the screen adds: drink water, and think about the sun.

Rain and snow come next, based on the kind of weather being reported right now:

- Freezing rain: a warning that surfaces may be slippery.
- Snow: a warm coat and waterproof shoes.
- Heavy rain: an umbrella and a rain jacket.
- Ordinary rain, drizzle, or showers: take an umbrella.
- If the report says rain is actually falling, even under a vague label: take an umbrella.
- Otherwise: no rain just now.

Sun strength uses the UV index, the same scale weather apps use:

- 0 to 2: UV is low.
- 3 to 5: sunscreen.
- 6 to 7: sunscreen, sunglasses, and shade.
- 8 and above: strong protection, and spend less time in the sun.

Writing this list first meant the screen could only say things we had already decided. A cold morning cannot accidentally be described as a light jacket.

## Step 2. We borrowed a screen that was already known to work

The folder we started in was empty. The risky part of a project like this is the screen: if the picture never appears, you cannot tell whether the weather idea failed or the panel was simply set up wrong.

So we looked at two projects that already run on this kind of hardware. One is a Game of Life demo, useful for the general look and feel of a full-screen Rust demo on this board. The other is a set of examples written specifically for the ESP32-S31-Korvo-1. From that second project we took the screen setup as it stood: the same pins, the same timing, the same way the picture is held in the board’s extra memory and swept onto the panel.

That choice kept the hardware story boring, which is what we wanted. If you already have the screen attached the way those examples expect, this advisor uses that same arrangement.

## Step 3. We taught it to join Wi-Fi and stay joined

The weather lives on the internet, so the first job after the screen wakes up is the network.

The Wi-Fi name and password are baked in when the program is prepared for the board. After that, the board does not ask you to type them. On startup the screen says it is starting up. In the background it joins the network, asks the router for an address, and only then goes looking for the weather.

If the connection drops later, it tries again. The radio stays fully awake, because this board is meant to sit there all day, not to sleep between glances.

## Step 4. We asked a free weather service, in two small questions

Open-Meteo will answer a plain web request. We did not need a key or a login.

The board asks two questions, in order:

1. “Where is Brno?” The geocoding service replies with a latitude and longitude. We keep that answer, so we only look the city up once.
2. “What is the weather there right now?” The forecast service replies with five facts: the air temperature, a standard weather code (clear, rain, snow, fog, thunder, and so on), how much precipitation fell recently, the UV index, and whether it is currently day or night.

The city name can be changed when the program is built. If you leave it alone, it is Brno.

Each request is given about half a minute. If the service is silent, or the reply cannot be read, the board does not freeze on a blank screen. It says there is a connection problem, waits half a minute, and tries again.

## Step 5. We turned those facts into the sentences from step 1

The service speaks in numbers and short codes. A person standing by the door needs a sentence.

Temperature becomes one of the coat lines. The weather code becomes a short condition, such as “Clear sky”, “Overcast”, “Light rain”, “Snow”, or “Thunderstorm”. The same code, plus the amount of precipitation, picks the umbrella or snow line. The UV number picks the sun line. Day or night does not change the clothing advice. It changes the picture behind the words: a bright sky in the daytime, a dark sky with a moon after dark.

We checked these rules on an ordinary computer with sample weather reports, before trusting them on the board. A freezing report, a rainy report, a hot afternoon, and a low-UV evening each had to produce the sentence we had written down in step 1.

## Step 6. We laid the screen out like a phone weather app

The panel is large enough for a few lines of large text and a picture behind them. Every screen shares the same frame:

- A bar across the top reads “WEATHER & OUTFIT ADVISOR”, with the city name on the right.
- Under that, a panel holds the message.

When the weather has arrived, you see:

- The temperature, and beside it the condition in words.
- How much rain or snow has been reported.
- An “Outfit” line.
- An extra orange line when it is hot enough to mention water and sun.
- An “Alert” line, either the umbrella or snow warning, or “No rain now”.
- A “UV” line with the index and the matching advice.

Behind the words, the sky matches the report. Clear weather shows sun or moon and stars. Clouds, fog, rain, and snow each get their own sky. Rain is drawn as short streaks, from a drizzle up to a storm. Snow drifts. Thunderstorms flash. The words stay readable while that weather moves across them.

The picture is revealed in bands, so the panel fills in smoothly instead of flickering. Once the weather is on screen, the rain, snow, or lightning keeps moving until the next update.

If something goes wrong, the same frame is used for a calm error: “Connection problem”, a short reason, and “Retrying…”.

## Step 7. We made the update automatic

The plan was that the advice should stay current without anyone touching the board.

After a successful reading, the board waits 30 minutes, still showing the animated sky, then asks Open-Meteo again. After a failed reading, it waits 30 seconds and tries again. That loop never ends. You can leave it by the door.

It is a picture of the weather right now, refreshed on that schedule. It is not a week-long forecast, and it does not buzz your phone. The value is the sentence on the screen when you pick up your keys.

## Step 8. We loaded it onto the board

Preparing the program and copying it onto the board is a one-time job for whoever sets the device up. In plain terms:

- The program is built for this chip.
- The Wi-Fi name and password are supplied at that moment, along with the city if it should not be Brno.
- The result is copied over a USB cable into the board’s programming port, with the power switch on.
- A serial monitor can be opened if you want a behind-the-scenes log of “joining Wi-Fi” and “here is the temperature”. The screen does not depend on that log.

If the copy fails once with a broken connection, trying again is often enough. If it keeps failing, holding Boot, tapping Reset, and releasing Boot puts the board into the mode where it accepts a new program.

There is also a separate screen test. It draws a storm, the busiest picture we use, and leaves it running. If that test is blank, the trouble is the panel, the power, or the cable. If the test is fine and the advisor is not, the trouble is the network or the weather request.

## A morning with it

You walk past the board. The top bar still says Brno. The sky is grey and rain streaks are moving. The large line says 4 °C and “Rain”. Under Outfit: “Warm coat”. Under Alert: “Take an umbrella”. UV is low, because it is a wet morning.

On a different day the sky is bright, the line says 27 °C and “Clear sky”, the outfit is light clothes, and an extra line tells you to drink water and think about the sun. The UV line asks for sunscreen. No umbrella.

If the office Wi-Fi blips, you see “Connection problem” and “Retrying…” instead of a frozen old picture. When the network returns, the next successful ask replaces that message with the weather again.

## What you are left with

A Korvo-1, a screen, a Wi-Fi network, and a free weather service. No extra parts. The advice is a fixed set of sentences, so the board cannot invent a strange recommendation. It looks up Brno, explains the current weather in those sentences, draws a sky to match, and asks again every half hour.

That is the whole device: a note by the door, written by a small computer, about what to put on before you go out.
