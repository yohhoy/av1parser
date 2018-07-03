#!/usr/bin/env python3
"""
Download ivf files (aom test vectors)

https://aomedia.googlesource.com/aom/
"""
import os.path
from lxml import etree
import requests

URL = 'http://storage.googleapis.com/aom-test-data/'
XMLNS = 'http://doc.s3.amazonaws.com/2006-03-01'


def download_file(fname):
    res = requests.get(URL + fname, stream=True)
    if not res.status_code == 200:
        print(f'{fname} {res.status_code}')
        return
    with open(fname, 'wb') as f:
        for chunk in res.iter_content():
            f.write(chunk)
    length = res.headers['Content-length']
    print(f'{fname} done({length})')


def download_ivfs():
    xml = etree.fromstring(requests.get(URL).content)
    ivf_list = xml.xpath('//s3:Key/text()', namespaces={'s3':XMLNS})
    for fname in ivf_list:
        if not fname.endswith('.ivf'):
            continue
        if os.path.exists(fname):
            print(f'{fname} skipped')
            continue
        download_file(fname)


if __name__ == '__main__':
    download_ivfs()
