#!/usr/bin/env python3
"""
Download MPEG-DASH contents
"""
import math
import urllib.parse
import re
from lxml import etree
import requests

XMLNS = {'ns': 'urn:mpeg:dash:schema:mpd:2011'}


def parse_mpd(url):
    """parse MPD meta"""
    mpd = etree.fromstring(requests.get(url).content)
    duration = mpd.xpath(
        '/ns:MPD/@mediaPresentationDuration', namespaces=XMLNS)[0]
    video_set = mpd.xpath(
        '//ns:AdaptationSet[@mimeType="video/webm"]/ns:Representation', namespaces=XMLNS)
    videos = []
    for node in video_set:
        rep = {}
        for attr in ['bandwidth', 'width', 'height', 'frameRate', 'codecs']:
            rep[attr] = node.xpath(f'@{attr}')[0]
        for attr in ['media', 'initialization', 'duration', 'startNumber', 'timescale']:
            rep[attr] = node.xpath(
                f'ns:SegmentTemplate/@{attr}', namespaces=XMLNS)[0]
        rep['media'] = urllib.parse.urljoin(url, rep['media'])
        rep['initialization'] = urllib.parse.urljoin(
            url, rep['initialization'])
        videos.append(rep)
    return {'duration': duration, 'media': videos}


def download_media(track, chunks, fname):
    """download media chunks to local file"""
    with open(fname, 'wb') as file:
        print(f'{track["initialization"]}')
        init_seg = requests.get(track['initialization'])
        fsize = file.write(init_seg.content)
        seg = int(track['startNumber'])
        while chunks > 0:
            media_url = track["media"].replace('$Number$', str(seg))
            print(f'{media_url}', end='\r')
            media_seg = requests.get(media_url)
            fsize += file.write(media_seg.content)
            seg += 1
            chunks -= 1
        print(f'{media_url}')
    print(f'write {fsize} bytes')


def parse_duration(duration):
    """parse ISO 8601 duration('PnYnMnDTnHnMnS')"""
    match = re.match(r'P\d+Y\d+M\d+DT(\d+)H(\d+)M(\d+.?\d*)S', duration)
    hms = match.groups()
    return int(hms[0]) * 3600 + int(hms[1]) * 60 + float(hms[2])


def download_mpd(mpdurl, rep_idx, fname):
    """download MPEG-DASH contents"""
    mpd = parse_mpd(mpdurl)
    duration = parse_duration(mpd['duration'])
    print(f'URL: {mpdurl}')
    print(f'MPD duration={duration}')
    for i, rep in enumerate(mpd['media']):
        mark = '*' if i == rep_idx else ' '
        print('{}{}: res={width}x{height} fps={frameRate} codec="{codecs}" br={bandwidth}'
              .format(mark, i, **rep))
    track = mpd['media'][rep_idx]
    chunks = math.ceil(
        duration / (float(track['duration']) / float(track['timescale'])))
    print(f'download #{rep_idx} ({chunks} chunks) to "{fname}"')
    download_media(mpd['media'][rep_idx], chunks, fname)


MPD_URL = 'https://bitmovin-a.akamaihd.net/webpages/demos/content/av1/chrome/stream.mpd'
REP_IDX = 0
download_mpd(MPD_URL, REP_IDX, 'fetchmpd.webm')
